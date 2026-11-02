//! Studio-console design: charcoal surfaces, recessed instruments, and restrained
//! periwinkle/rose light. Color denotes the channel; status uses a separate LED.
use eframe::egui::{self, Color32, RichText};
pub const BACKGROUND: Color32 = Color32::from_rgb(20, 20, 23);
pub const PANEL: Color32 = Color32::from_rgb(31, 31, 35);
pub const INSET: Color32 = Color32::from_rgb(22, 22, 26);
pub const BORDER: Color32 = Color32::from_rgb(51, 51, 58);
pub const TEXT: Color32 = Color32::from_rgb(239, 239, 245);
pub const MUTED: Color32 = Color32::from_rgb(160, 160, 174);
pub const LEFT: Color32 = Color32::from_rgb(149, 157, 255);
pub const RIGHT: Color32 = Color32::from_rgb(238, 145, 191);
pub const SUCCESS: Color32 = Color32::from_rgb(144, 210, 157);
pub fn alpha(color: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}
pub fn configure(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.spacing.item_spacing = egui::vec2(10., 10.);
    style.spacing.button_padding = egui::vec2(12., 7.);
    style.spacing.interact_size = egui::vec2(28., 28.);
    style.spacing.slider_rail_height = 4.;
    style.spacing.slider_width = 150.;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = INSET;
    style.visuals.faint_bg_color = Color32::from_rgb(37, 37, 42);
    style.visuals.selection.bg_fill = Color32::from_rgb(70, 66, 111);
    style.visuals.selection.stroke = egui::Stroke::new(1., TEXT);
    style.visuals.window_corner_radius = 12.into();
    style.visuals.window_stroke = egui::Stroke::new(1., BORDER);
    style.visuals.slider_trailing_fill = true;
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = 8.into();
        widget.fg_stroke = egui::Stroke::new(1.2, TEXT);
        widget.bg_stroke = egui::Stroke::new(1., BORDER);
    }
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(44, 44, 50);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(39, 39, 44);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(57, 55, 69);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(50, 48, 61);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(68, 61, 91);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(61, 54, 83);
    style.visuals.widgets.open.bg_fill = Color32::from_rgb(56, 50, 76);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1., BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1., MUTED);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.));
    ctx.set_style(style);
}
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(16.)
        .inner_margin(16.)
        .stroke(egui::Stroke::new(1., BORDER))
        .shadow(egui::epaint::Shadow {
            offset: [0, 5],
            blur: 15,
            spread: 0,
            color: Color32::from_black_alpha(65),
        })
}
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(10.).strong().color(MUTED));
}
pub fn led(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10., 10.), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 5., alpha(color, 20));
    ui.painter().circle_filled(rect.center(), 2.5, color);
}
pub fn pill_toggle(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: &str,
    accent: Color32,
) -> egui::Response {
    let font = egui::FontId::proportional(12.);
    let galley = ui.painter().layout_no_wrap(label.into(), font, TEXT);
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(galley.size().x + 58., 31.), egui::Sense::click());
    if response.clicked() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space)))
    {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    let p = ui.painter();
    let color = if ui.is_enabled() { accent } else { MUTED };
    p.rect_filled(
        rect,
        16.,
        if *value {
            alpha(color, 27)
        } else {
            Color32::from_rgb(38, 38, 43)
        },
    );
    p.rect_stroke(
        rect,
        16.,
        egui::Stroke::new(1., if *value { alpha(color, 90) } else { BORDER }),
        egui::StrokeKind::Inside,
    );
    let track = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 23., rect.center().y),
        egui::vec2(28., 15.),
    );
    p.rect_filled(
        track,
        8.,
        if *value {
            color
        } else {
            Color32::from_rgb(16, 16, 20)
        },
    );
    p.circle_filled(
        egui::pos2(
            if *value {
                track.right() - 7.5
            } else {
                track.left() + 7.5
            },
            track.center().y,
        ),
        5.5,
        if *value { TEXT } else { MUTED },
    );
    p.galley(
        egui::pos2(rect.left() + 45., rect.center().y - galley.size().y / 2.),
        galley,
        TEXT,
    );
    response
}
pub fn rotary(ui: &mut egui::Ui, value: &mut f64, accent: Color32) -> egui::Response {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(76., 76.), egui::Sense::click_and_drag());
    if response.dragged() {
        *value = (*value + (response.drag_delta().x - response.drag_delta().y) as f64 * 0.22)
            .clamp(-60., 12.);
        response.mark_changed();
    }
    if response.double_clicked() {
        *value = -6.;
        response.mark_changed();
    }
    if response.has_focus() && ui.is_enabled() {
        let step = ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight) {
                0.5
            } else if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft) {
                -0.5
            } else {
                0.
            }
        });
        if step != 0. {
            *value = (*value + step).clamp(-60., 12.);
            response.mark_changed();
        }
    }
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), *value, "Preamp"));
    let p = ui.painter();
    let center = rect.center();
    let start = 135f32.to_radians();
    let sweep = 270f32.to_radians();
    let fraction = ((*value + 60.) / 72.).clamp(0., 1.) as f32;
    for i in 0..=24 {
        let angle = start + sweep * i as f32 / 24.;
        let d = egui::vec2(angle.cos(), angle.sin());
        p.line_segment(
            [center + d * 32., center + d * 35.],
            egui::Stroke::new(
                1.,
                if i as f32 / 24. <= fraction {
                    alpha(accent, 160)
                } else {
                    Color32::from_gray(60)
                },
            ),
        );
    }
    p.circle_filled(
        center + egui::vec2(0., 3.),
        29.,
        Color32::from_black_alpha(100),
    );
    p.circle_filled(center, 28., Color32::from_rgb(12, 12, 15));
    p.circle_filled(center, 25., Color32::from_rgb(44, 44, 50));
    p.circle_stroke(
        center,
        24.,
        egui::Stroke::new(1., Color32::from_rgb(67, 67, 74)),
    );
    let points = (0..=60)
        .map(|i| {
            let angle = start + sweep * fraction * i as f32 / 60.;
            center + egui::vec2(angle.cos(), angle.sin()) * 28.
        })
        .collect();
    p.add(egui::Shape::line(
        points,
        egui::Stroke::new(2., alpha(accent, 190)),
    ));
    let angle = start + sweep * fraction;
    let d = egui::vec2(angle.cos(), angle.sin());
    p.line_segment(
        [center + d * 11., center + d * 19.],
        egui::Stroke::new(2.5, TEXT),
    );
    p.circle_filled(center + d * 19., 2., TEXT);
    response.on_hover_text("Preamp · drag up/right to raise level, down/left to lower. Double-click for −6 dB. Arrow keys adjust 0.5 dB.")
}
pub fn meter(ui: &mut egui::Ui, peak: f32, accent: Color32) {
    let db = 20. * peak.max(1e-6).log10();
    ui.horizontal(|ui| {
        caption(ui, "OUTPUT");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(if peak > 1e-6 {
                    format!("{db:.1} dBFS")
                } else {
                    "— dBFS".into()
                })
                .monospace()
                .size(11.)
                .color(MUTED),
            );
        });
    });
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 15.), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 5., INSET);
    let inside = rect.shrink(3.);
    let count = 32;
    let active = (((db + 60.) / 60.).clamp(0., 1.) * count as f32).ceil() as usize;
    for i in 0..count {
        let x = inside.left() + inside.width() * i as f32 / count as f32;
        let w = (inside.width() / count as f32 - 2.).max(1.);
        p.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, inside.top()), egui::vec2(w, inside.height())),
            1.,
            if i < active {
                if i > 29 {
                    Color32::from_rgb(239, 153, 130)
                } else {
                    accent
                }
            } else {
                Color32::from_rgb(44, 44, 50)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 300.),
            )),
            events,
            ..Default::default()
        }
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn new_toggle_accepts_clicks_but_not_when_disabled() {
        for enabled in [true, false] {
            let ctx = egui::Context::default();
            let mut value = false;
            let draw = |events: Vec<egui::Event>, value: &mut bool| {
                let _ = ctx.run(input(events), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            pill_toggle(ui, value, "EQ", LEFT);
                        });
                    });
                });
            };
            draw(vec![], &mut value);
            let pos = egui::pos2(35., 22.);
            draw(
                vec![egui::Event::PointerMoved(pos), button(pos, true)],
                &mut value,
            );
            draw(vec![button(pos, false)], &mut value);
            assert_eq!(value, enabled);
        }
    }
    #[test]
    fn rotary_drag_caps_gain_to_valid_profile_range() {
        let ctx = egui::Context::default();
        let mut value = -6.;
        let draw = |events: Vec<egui::Event>, value: &mut f64| {
            let _ = ctx.run(input(events), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rotary(ui, value, LEFT);
                });
            });
        };
        draw(vec![], &mut value);
        let pos = egui::pos2(45., 45.);
        draw(
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
            &mut value,
        );
        draw(
            vec![egui::Event::PointerMoved(egui::pos2(45., -1000.))],
            &mut value,
        );
        assert_eq!(value, 12.);
        draw(
            vec![egui::Event::PointerMoved(egui::pos2(45., 1000.))],
            &mut value,
        );
        assert_eq!(value, -60.);
    }
}
