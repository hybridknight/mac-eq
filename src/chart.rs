use crate::{
    dsp,
    profile::{Band, Channel, Filter, MAX_BANDS},
    spectrum::{BINS, FFT_SIZE},
    theme,
};
use eframe::egui::{self, Color32, RichText};

fn has_gain(kind: Filter) -> bool {
    !matches!(kind, Filter::LowPass | Filter::HighPass)
}
fn coordinates(pos: egui::Pos2, plot: egui::Rect, max_hz: f64) -> (f64, f64) {
    let fraction = ((pos.x - plot.left()) / plot.width()).clamp(0., 1.);
    (
        20. * (max_hz / 20.).powf(fraction as f64),
        ((plot.center().y - pos.y) as f64 / (plot.height() / 2.) as f64 * 24.).clamp(-24., 24.),
    )
}
pub fn response_graph(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    bypass: bool,
    rate: f64,
    color: Color32,
    spectrum: Option<&[f32; BINS]>,
) -> bool {
    let rate = if rate.is_finite() && rate >= 8000. {
        rate
    } else {
        48000.
    };
    let max_hz = 20000f64.min(rate * 0.49);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 200.),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8., Color32::from_rgb(15, 15, 18));
    let plot = rect.shrink2(egui::vec2(33., 30.));
    painter.text(
        rect.right_top() + egui::vec2(-8., 8.),
        egui::Align2::RIGHT_TOP,
        "LEVEL / dBFS",
        egui::FontId::proportional(9.),
        Color32::GRAY,
    );
    let x =
        |hz: f64| plot.left() + ((hz / 20.).log10() / (max_hz / 20.).log10()) as f32 * plot.width();
    let y = |db: f64| plot.center().y - db as f32 / 24. * plot.height() / 2.;
    let spectrum_y = |db: f32| plot.bottom() - (db.clamp(-90., 0.) + 90.) / 90. * plot.height();
    let muted = theme::MUTED;
    painter.text(
        rect.left_top() + egui::vec2(10., 8.),
        egui::Align2::LEFT_TOP,
        if spectrum.is_some() {
            "LIVE SPECTRUM"
        } else {
            "SPECTRUM · WAITING FOR AUDIO"
        },
        egui::FontId::proportional(10.),
        muted,
    );
    // Fine dotted texture borrows the reference's instrument-panel grid.
    for row in 0..=(plot.height() / 15.) as usize {
        for col in 0..=(plot.width() / 15.) as usize {
            painter.circle_filled(
                plot.left_top() + egui::vec2(col as f32 * 15., row as f32 * 15.),
                0.6,
                Color32::from_gray(39),
            );
        }
    }
    // Logarithmic columns show bin peaks. The right-hand scale is dBFS;
    // the EQ response and control dots use the left-hand gain scale.
    if let Some(levels) = spectrum {
        for column in 0..100 {
            let lo = 20. * (max_hz / 20.).powf(column as f64 / 100.);
            let hi = 20. * (max_hz / 20.).powf((column + 1) as f64 / 100.);
            let start = ((lo / rate * FFT_SIZE as f64).floor() as usize).clamp(1, BINS - 1);
            let end = ((hi / rate * FFT_SIZE as f64).ceil() as usize).clamp(start + 1, BINS);
            let peak = levels[start..end].iter().copied().fold(-100f32, f32::max);
            let top = spectrum_y(peak);
            if top < plot.bottom() {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x(lo), top),
                        egui::pos2(x(hi), plot.bottom()),
                    ),
                    0.,
                    theme::alpha(color, 24),
                );
                painter.line_segment(
                    [egui::pos2(x(lo), top), egui::pos2(x(hi), top)],
                    egui::Stroke::new(1., theme::alpha(color, 90)),
                );
            }
        }
    }
    for hz in [
        20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000.,
    ] {
        if hz > max_hz {
            continue;
        }
        painter.line_segment(
            [
                egui::pos2(x(hz), plot.top()),
                egui::pos2(x(hz), plot.bottom()),
            ],
            egui::Stroke::new(1., Color32::from_gray(29)),
        );
        // At narrow widths keep every other label, while preserving grid lines.
        if plot.width() < 550. && [50., 200., 2000., 5000.].contains(&hz) {
            continue;
        }
        painter.text(
            egui::pos2(x(hz), plot.bottom() + 5.),
            egui::Align2::CENTER_TOP,
            if hz >= 1000. {
                format!("{}k", hz / 1000.)
            } else {
                format!("{hz}")
            },
            egui::FontId::proportional(10.),
            muted,
        );
    }
    for db in [-24., -12., 0., 12., 24.] {
        painter.line_segment(
            [
                egui::pos2(plot.left(), y(db)),
                egui::pos2(plot.right(), y(db)),
            ],
            egui::Stroke::new(1., Color32::from_gray(if db == 0. { 65 } else { 30 })),
        );
        painter.text(
            egui::pos2(plot.left() - 5., y(db)),
            egui::Align2::RIGHT_CENTER,
            format!("{db:+}"),
            egui::FontId::proportional(9.),
            muted,
        );
    }
    for db in [0., -30., -60., -90.] {
        painter.text(
            egui::pos2(plot.right() + 4., spectrum_y(db)),
            egui::Align2::LEFT_CENTER,
            format!("{db:.0}"),
            egui::FontId::proportional(8.),
            muted,
        );
    }
    let coefficients: Vec<_> = channel
        .bands
        .iter()
        .map(|b| dsp::Coeff::design(b, rate))
        .collect();
    let points: Vec<egui::Pos2> = (0..240)
        .map(|i| {
            let hz = 20. * (max_hz / 20.).powf(i as f64 / 239.);
            let db = if bypass {
                0.
            } else {
                channel.preamp
                    + coefficients
                        .iter()
                        .map(|b| 20. * b.magnitude(hz, rate).max(1e-12).log10())
                        .sum::<f64>()
            };
            egui::pos2(x(hz), y(db.clamp(-24., 24.)))
        })
        .collect();
    let curve_color = if bypass { muted } else { color };
    let mut fill = egui::epaint::Mesh::default();
    for point in &points {
        fill.vertices.push(egui::epaint::Vertex {
            pos: *point,
            uv: egui::epaint::WHITE_UV,
            color: theme::alpha(curve_color, 22),
        });
        fill.vertices.push(egui::epaint::Vertex {
            pos: egui::pos2(point.x, y(0.)),
            uv: egui::epaint::WHITE_UV,
            color: theme::alpha(curve_color, 0),
        });
    }
    for i in 0..points.len() - 1 {
        let j = (i * 2) as u32;
        fill.indices
            .extend_from_slice(&[j, j + 1, j + 2, j + 1, j + 2, j + 3]);
    }
    painter.add(egui::Shape::mesh(fill));
    for (width, a) in [(9., 12), (5., 25)] {
        painter.add(egui::Shape::line(
            points.clone(),
            egui::Stroke::new(width, theme::alpha(curve_color, a)),
        ));
    }
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(2.2, curve_color),
    ));
    let id = ui.id().with("selected_chart_band");
    let mut selected = ui
        .data_mut(|data| data.get_temp::<usize>(id))
        .filter(|i| *i < channel.bands.len());
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    // Select from where the press began, not where the first drag frame lands.
    // Fast pointer movement must keep hold of the original dot.
    let selection_pointer = if response.drag_started() {
        ui.input(|input| input.pointer.press_origin())
    } else {
        pointer
    };
    let nearest = selection_pointer.and_then(|pos| {
        channel
            .bands
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let point = egui::pos2(
                    x(b.hz.min(max_hz)),
                    y(if has_gain(b.kind) { b.db } else { 0. }),
                );
                (i, pos.distance(point))
            })
            .filter(|(_, distance)| *distance <= 18.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    });
    let mut changed = false;
    if response.drag_started() || response.clicked() {
        if let Some(pos) = selection_pointer.filter(|p| plot.contains(*p)) {
            selected = nearest;
            if selected.is_none() && channel.bands.len() < MAX_BANDS {
                let (hz, db) = coordinates(pos, plot, max_hz);
                selected = Some(channel.bands.len());
                channel.bands.push(Band {
                    hz,
                    db,
                    ..Band::default()
                });
                changed = true;
            }
        }
    }
    if response.dragged() {
        if let (Some(i), Some(pos)) = (selected, pointer) {
            let band = &mut channel.bands[i];
            if ui.input(|input| input.modifiers.shift) {
                band.q = (band.q * (-response.drag_delta().y as f64 * 0.02).exp()).clamp(0.1, 20.);
            } else {
                let (hz, db) = coordinates(pos, plot, max_hz);
                band.hz = hz;
                if has_gain(band.kind) {
                    band.db = db;
                }
            }
            band.enabled = true;
            changed = true;
        }
    }
    if response.double_clicked() {
        if let Some(i) = nearest {
            channel.bands[i].db = 0.;
            selected = Some(i);
            changed = true;
        }
    }
    if let Some(i) = selected {
        ui.data_mut(|data| data.insert_temp(id, i));
    } else {
        ui.data_mut(|data| data.remove::<usize>(id));
    }
    for (i, band) in channel.bands.iter().enumerate() {
        let pos = egui::pos2(
            x(band.hz.min(max_hz)),
            y(if has_gain(band.kind) { band.db } else { 0. }),
        );
        let dot_color = if band.enabled { color } else { muted };
        painter.circle_filled(
            pos,
            if selected == Some(i) { 13. } else { 10. },
            theme::alpha(dot_color, 30),
        );
        painter.circle_stroke(
            pos,
            if selected == Some(i) { 8. } else { 6.5 },
            egui::Stroke::new(1.5, theme::alpha(dot_color, 180)),
        );
        painter.circle_filled(
            pos,
            if selected == Some(i) { 5.5 } else { 4. },
            if band.enabled { theme::TEXT } else { muted },
        );
        // Overlapping bands remain individually accessible in the band cards.
        if selected == Some(i)
            || !channel.bands[..i]
                .iter()
                .any(|b| (x(b.hz.min(max_hz)) - pos.x).abs() < 20. && (y(b.db) - pos.y).abs() < 18.)
        {
            painter.text(
                pos + egui::vec2(0., -11.),
                egui::Align2::CENTER_BOTTOM,
                format!("{}", i + 1),
                egui::FontId::proportional(10.),
                color,
            );
        }
    }
    if response.hovered() || response.dragged() {
        if let Some(pos) = pointer.filter(|p| plot.contains(*p)) {
            let (hz, db) = coordinates(pos, plot, max_hz);
            painter.line_segment(
                [
                    egui::pos2(pos.x, plot.top()),
                    egui::pos2(pos.x, plot.bottom()),
                ],
                egui::Stroke::new(1., Color32::from_gray(85)),
            );
            response.on_hover_ui(|ui| {
                ui.label(format!("{hz:.0} Hz · {db:+.1} dB gain"));
                if let Some(levels) = spectrum {
                    let bin = ((hz / rate * FFT_SIZE as f64).round() as usize).min(BINS - 1);
                    ui.label(format!("Playing: {:.1} dBFS", levels[bin]));
                }
                ui.label(RichText::new("Drag: frequency / gain · Shift-drag: width").small());
                ui.label(
                    RichText::new("Click empty space: add · Double-click dot: reset gain").small(),
                );
            });
        }
    }
    changed
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chart_maps_log_frequency_and_clamps_gain() {
        let plot = egui::Rect::from_min_max(egui::pos2(10., 20.), egui::pos2(410., 220.));
        let (hz, db) = coordinates(plot.center(), plot, 20000.);
        assert!((hz - 632.4555).abs() < 0.01);
        assert_eq!(db, 0.);
        assert_eq!(
            coordinates(egui::pos2(-100., -100.), plot, 20000.),
            (20., 24.)
        );
        let (hz, db) = coordinates(egui::pos2(1000., 1000.), plot, 20000.);
        assert!((hz - 20000.).abs() < 1e-8);
        assert_eq!(db, -24.);
    }
    #[test]
    fn pointer_click_adds_a_band_and_drag_changes_frequency_and_gain() {
        let ctx = egui::Context::default();
        let mut channel = Channel {
            preamp: 0.,
            bands: Vec::new(),
        };
        let draw = |events: Vec<egui::Event>, channel: &mut Channel| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 400.),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    response_graph(ui, channel, false, 48000., Color32::WHITE, None);
                });
            });
        };
        draw(vec![], &mut channel);
        let pos = egui::pos2(300., 110.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        draw(
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
            &mut channel,
        );
        draw(vec![button(pos, false)], &mut channel);
        assert_eq!(channel.bands.len(), 1);
        let original_hz = channel.bands[0].hz;
        let target = pos + egui::vec2(60., -30.);
        draw(vec![button(pos, true)], &mut channel);
        draw(vec![egui::Event::PointerMoved(target)], &mut channel);
        draw(vec![button(target, false)], &mut channel);
        assert!(channel.bands[0].hz > original_hz * 1.5);
        assert!(channel.bands[0].db > 5.);
    }
}
