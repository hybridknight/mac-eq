use crate::{
    audio,
    chart::response_graph,
    profile::{Band, Channel, Filter, MAX_BANDS},
    theme::{self, LEFT, MUTED, RIGHT},
    MacEQ,
};
use eframe::egui::{self, Color32, RichText};
use std::time::{Duration, Instant};
fn filter_name(kind: Filter) -> &'static str {
    match kind {
        Filter::Peak => "Bell",
        Filter::LowShelf => "Low shelf",
        Filter::HighShelf => "High shelf",
        Filter::LowPass => "Low pass",
        Filter::HighPass => "High pass",
    }
}
fn has_gain(kind: Filter) -> bool {
    !matches!(kind, Filter::LowPass | Filter::HighPass)
}
impl MacEQ {
    fn save_indicator(&self, ui: &mut egui::Ui) {
        let (label, color) = if !self.can_save || !self.save_error.is_empty() {
            ("Save error", RIGHT)
        } else if self.dirty {
            ("Saving…", MUTED)
        } else {
            ("Autosaved", theme::SUCCESS)
        };
        egui::Frame::new()
            .fill(theme::INSET)
            .corner_radius(14.)
            .inner_margin(egui::Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::led(ui, color);
                    ui.label(RichText::new(label).size(11.).color(MUTED));
                });
            })
            .response
            .on_hover_text("Every EQ edit saves automatically for this device.");
    }
    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Settings", |ui| {
            theme::caption(ui, "PREFERENCES");
            let mut login = matches!(self.login_status, 1 | 2);
            if theme::pill_toggle(ui, &mut login, "Open at login", LEFT).changed() {
                self.set_login(login);
            }
            if self.login_status == 2 {
                ui.label("Approval needed in macOS Login Items.");
                if ui.button("Open Login Items settings").clicked() {
                    crate::platform::open_login_settings();
                }
            }
            if self.login_status == 3 {
                ui.label("Run the signed MacEQ.app to manage login startup.");
            }
            if !self.login_error.is_empty() {
                ui.colored_label(RIGHT, &self.login_error);
            }
            ui.separator();
            ui.label(
                RichText::new("Closing the window hides the Dock icon. EQ stays in the menu bar.")
                    .small()
                    .color(MUTED),
            );
            if ui.button("Quit MacEQ").clicked() {
                self.quitting = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(icon) = &self.app_icon {
                ui.add(egui::Image::new((icon.id(), egui::vec2(43., 43.))).corner_radius(10.));
            }
            ui.vertical(|ui| {
                ui.label(RichText::new("MacEQ").size(25.).strong());
                theme::caption(ui, "SYSTEM AUDIO · PARAMETRIC EQUALIZER");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.settings_menu(ui);
                self.save_indicator(ui);
                let active = self
                    .sessions
                    .values()
                    .filter(|s| s.engine.is_some())
                    .count();
                ui.label(
                    RichText::new(format!("{active} active"))
                        .size(11.)
                        .color(MUTED),
                );
            });
        });
        ui.add_space(13.);
        self.device_strip(ui);
    }
    fn device_strip(&mut self, ui: &mut egui::Ui) {
        theme::card().inner_margin(12.).show(ui, |ui| {
            let mut selected = self.settings.selected.clone();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    theme::caption(ui, "OUTPUT DEVICE");
                    let name = self
                        .settings
                        .device_names
                        .get(&selected)
                        .map(String::as_str)
                        .or_else(|| {
                            audio::target(&selected, &self.devices).map(|d| d.name.as_str())
                        })
                        .unwrap_or(if selected.is_empty() {
                            "Choose an output"
                        } else {
                            "Saved device · disconnected"
                        });
                    egui::ComboBox::from_id_salt("device")
                        .selected_text(name)
                        .width(305.)
                        .show_ui(ui, |ui| {
                            for row in crate::platform::device_rows(&self.settings, &self.devices) {
                                let uid = row["uid"].as_str().unwrap_or_default();
                                let name = row["name"].as_str().unwrap_or_default();
                                let suffix = if row["connected"].as_bool() == Some(true) {
                                    ""
                                } else {
                                    " · disconnected"
                                };
                                ui.selectable_value(
                                    &mut selected,
                                    uid.into(),
                                    format!("{name}{suffix}"),
                                );
                            }
                        });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = self.settings.is_enabled(&self.settings.selected);
                    ui.add_enabled_ui(
                        !self.settings.selected.is_empty() && !self.cleanup_failed,
                        |ui| {
                            if theme::pill_toggle(ui, &mut enabled, "EQ enabled", LEFT).changed() {
                                if enabled {
                                    self.enable();
                                } else {
                                    self.stop();
                                }
                            }
                        },
                    );
                    ui.menu_button("Devices", |ui| {
                        ui.set_min_width(400.);
                        self.device_roster(ui);
                    });
                    ui.label(RichText::new(self.selected_status()).size(11.).color(MUTED));
                });
            });
            if selected != self.settings.selected {
                self.select_device(selected);
            }
        });
    }
    fn device_roster(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(
                "Enable only the outputs you want to equalize. Disabled devices remain untouched.",
            )
            .small()
            .color(MUTED),
        );
        egui::ScrollArea::vertical()
            .id_salt("device_list")
            .max_height(150.)
            .show(ui, |ui| {
                for row in crate::platform::device_rows(&self.settings, &self.devices) {
                    let uid = row["uid"].as_str().unwrap_or_default();
                    let name = row["name"].as_str().unwrap_or_default();
                    let connected = row["connected"].as_bool() == Some(true);
                    let mut enabled = self.settings.is_enabled(uid);
                    ui.horizontal(|ui| {
                        theme::led(ui, if connected { theme::SUCCESS } else { MUTED });
                        if ui
                            .selectable_label(self.settings.selected == uid, name)
                            .clicked()
                        {
                            self.select_device(uid.into());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::pill_toggle(ui, &mut enabled, "EQ", LEFT).changed() {
                                self.set_device_enabled(uid, enabled);
                            }
                            if !connected {
                                ui.label(RichText::new("Offline").small().color(MUTED));
                            }
                        });
                    });
                }
            });
    }
    fn shared_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.);
        let mut changed = false;
        ui.horizontal(|ui|{
            egui::Frame::new().fill(theme::INSET).corner_radius(18.).inner_margin(4.).show(ui,|ui|{
                ui.horizontal(|ui|{
                    ui.spacing_mut().item_spacing.x=4.;
                    changed|=ui.selectable_value(&mut self.profile.linked,true,"Linked L + R").on_hover_text("Edits on either side affect both channels. Linking copies the last edited side.").changed();
                    changed|=ui.selectable_value(&mut self.profile.linked,false,"Independent").on_hover_text("Adjust each channel separately.").changed();
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                changed|=theme::pill_toggle(ui,&mut self.profile.bypass,"Hear original",RIGHT).on_hover_text("Compare against dry audio without EQ.").changed();
                changed|=theme::pill_toggle(ui,&mut self.profile.limiter,"Clip protection",LEFT).on_hover_text("Reduces sample peaks before they exceed full scale.").changed();
            });
        });
        if changed {
            self.changed();
        }
        ui.add_space(8.);
    }
    fn channel_panel(&mut self, ui: &mut egui::Ui, ch: usize) {
        let accent = if ch == 0 { LEFT } else { RIGHT };
        let engine = self
            .sessions
            .get(&self.settings.selected)
            .and_then(|s| s.engine.as_ref());
        let rate = engine.map_or_else(
            || audio::target(&self.settings.selected, &self.devices).map_or(48000., |d| d.rate),
            |e| e.rate,
        );
        let peak = engine.map_or(0., |e| e.peaks()[ch]);
        let spectrum = engine.and_then(|e| e.spectrum.levels(ch));
        let bypass = self.profile.bypass;
        let channel = &mut self.profile.channels[ch];
        let mut changed = false;
        ui.push_id(ch,|ui|{
            theme::card().show(ui,|ui|{
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui|{
                    let(rect,_)=ui.allocate_exact_size(egui::vec2(30.,30.),egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(),15.,theme::alpha(accent,24));
                    ui.painter().circle_stroke(rect.center(),15.,egui::Stroke::new(1.,theme::alpha(accent,80)));
                    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,if ch==0{"L"}else{"R"},egui::FontId::proportional(14.),accent);
                    ui.label(RichText::new(if ch==0{"Left channel"}else{"Right channel"}).size(17.).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        ui.menu_button("•••",|ui|{
                            if ui.button("Reset channel").on_hover_text("Flat EQ, −6 dB preamp. Linked mode resets both.").clicked(){*channel=Channel::default();changed=true;ui.close_menu();}
                            if ui.button("Reset to 10 frequency bands").on_hover_text("Replaces current filters with ten flat bell bands. Linked mode changes both.").clicked(){channel.bands=[31.5,63.,125.,250.,500.,1000.,2000.,4000.,8000.,16000.].into_iter().map(|hz|Band{hz,q:1.4,..Default::default()}).collect();changed=true;ui.close_menu();}
                        });
                        ui.label(RichText::new(format!("{} / {MAX_BANDS}",channel.bands.len())).monospace().size(11.).color(MUTED));
                    });
                });
                ui.add_space(9.);changed|=response_graph(ui,channel,bypass,rate,accent,spectrum);
                ui.label(RichText::new("Click to add · Drag to shape · Shift-drag for width").size(10.).color(MUTED));
                ui.add_space(3.);
                ui.horizontal(|ui|{
                    changed|=theme::rotary(ui,&mut channel.preamp,accent).changed();
                    ui.vertical(|ui|{
                        ui.set_width(90.);theme::caption(ui,"PREAMP");
                        changed|=ui.add(egui::DragValue::new(&mut channel.preamp).range(-60. ..=12.).speed(0.2).fixed_decimals(1).suffix(" dB")).changed();
                        ui.label(RichText::new("Before EQ").size(10.).color(MUTED));
                    });
                    ui.separator();ui.vertical(|ui|theme::meter(ui,peak,accent));
                });
                ui.add_space(3.);
                let mut remove=None;
                let height=(ui.ctx().screen_rect().bottom()-ui.cursor().top()-100.).clamp(64.,230.);
                egui::ScrollArea::vertical().id_salt("bands").max_height(height).auto_shrink([false,false]).show(ui,|ui|{
                    if channel.bands.is_empty(){ui.label(RichText::new("Flat curve. Add a band to start shaping.").small().color(MUTED));}
                    for(i,band)in channel.bands.iter_mut().enumerate(){ui.push_id(i,|ui|{let(edit,deleted)=band_editor(ui,band,i,accent);changed|=edit;if deleted{remove=Some(i);}});ui.add_space(7.);}
                });
                if let Some(i)=remove{channel.bands.remove(i);changed=true;}
                ui.add_space(4.);
                if ui.add_enabled(channel.bands.len()<MAX_BANDS,egui::Button::new(RichText::new("+  Add frequency band").color(accent)).min_size(egui::vec2(ui.available_width(),31.))).clicked(){channel.bands.push(Band::default());changed=true;}
            });
        });
        if changed {
            self.channel = ch;
            self.changed();
        }
    }
    fn presets(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.);
        egui::CollapsingHeader::new("Preset library").show(ui, |ui| {
            ui.label(
                RichText::new("Device settings autosave. Keep extra versions here.")
                    .small()
                    .color(MUTED),
            );
            let mut load = None;
            ui.horizontal_wrapped(|ui| {
                for name in self.settings.presets.keys() {
                    if ui.button(name).clicked() {
                        load = Some(name.clone());
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.preset_name)
                        .hint_text("Name this sound…")
                        .desired_width(185.),
                );
                if ui
                    .add_enabled(
                        !self.preset_name.trim().is_empty(),
                        egui::Button::new("Store preset"),
                    )
                    .clicked()
                {
                    self.settings
                        .presets
                        .insert(self.preset_name.trim().into(), self.profile.clone());
                    self.dirty = true;
                    self.last_edit = Instant::now();
                }
                if ui
                    .add_enabled(
                        self.settings.presets.contains_key(self.preset_name.trim()),
                        egui::Button::new("Delete preset"),
                    )
                    .clicked()
                {
                    self.settings.presets.remove(self.preset_name.trim());
                    self.dirty = true;
                    self.last_edit = Instant::now();
                }
            });
            if let Some(name) = load {
                self.profile = self.settings.presets[&name].clone();
                self.preset_name = name;
                self.changed();
            }
        });
    }
}
fn band_editor(ui: &mut egui::Ui, band: &mut Band, index: usize, accent: Color32) -> (bool, bool) {
    let (mut changed, mut deleted) = (false, false);
    egui::Frame::new()
        .fill(theme::INSET)
        .corner_radius(10.)
        .inner_margin(10.)
        .stroke(egui::Stroke::new(1., Color32::from_rgb(42, 42, 48)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:02}", index + 1))
                        .monospace()
                        .size(11.)
                        .color(accent),
                );
                egui::ComboBox::from_id_salt("type")
                    .width(100.)
                    .selected_text(filter_name(band.kind))
                    .show_ui(ui, |ui| {
                        for kind in Filter::ALL {
                            changed |= ui
                                .selectable_value(&mut band.kind, kind, filter_name(kind))
                                .changed();
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Remove band").clicked() {
                        deleted = true;
                    }
                    changed |= theme::pill_toggle(ui, &mut band.enabled, "On", accent).changed();
                });
            });
            ui.add_enabled_ui(band.enabled, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.;
                    theme::caption(ui, "FREQ");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut band.hz)
                                .range(20. ..=20000.)
                                .speed(5.)
                                .max_decimals(1)
                                .suffix(" Hz"),
                        )
                        .changed();
                    theme::caption(ui, "Q");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut band.q)
                                .range(0.1..=20.)
                                .speed(0.02)
                                .max_decimals(2),
                        )
                        .on_hover_text("Lower Q is wider; higher Q is narrower.")
                        .changed();
                    ui.add_enabled_ui(has_gain(band.kind), |ui| {
                        ui.horizontal(|ui| {
                            theme::caption(ui, "GAIN");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut band.db)
                                        .range(-24. ..=24.)
                                        .speed(0.1)
                                        .max_decimals(1)
                                        .suffix(" dB"),
                                )
                                .changed();
                        });
                    });
                });
            });
        });
    (changed, deleted)
}
impl eframe::App for MacEQ {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "memory-profile")]
        self.memory_step(ctx);
        self.platform_events(ctx);
        let draw_editor =
            self.editor_visible && !ctx.input(|input| input.viewport().minimized.unwrap_or(false));
        if !self.quitting {
            self.tick(draw_editor);
        }
        if !draw_editor || self.quitting {
            // Keep device reconnection, autosave and menu commands alive without
            // constructing charts or uploading editor geometry in the background.
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(22.)).show(ctx,|ui|{
            egui::ScrollArea::vertical().id_salt("main").show(ui,|ui|{
                self.top_bar(ui);
                ui.add_enabled_ui(!self.settings.selected.is_empty(),|ui|{self.shared_controls(ui);ui.columns(2,|columns|{self.channel_panel(&mut columns[0],0);self.channel_panel(&mut columns[1],1);});self.presets(ui);});
                ui.add_space(7.);for error in [&self.error,&self.save_error]{if !error.is_empty(){ui.colored_label(RIGHT,error);}}
                ui.horizontal_wrapped(|ui|{theme::led(ui,theme::SUCCESS);ui.label(RichText::new("On-device processing · Autosave · Close to hide from Dock and keep EQ in the menu bar").size(10.).color(MUTED));});
            });
        });
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.close_engines();
        if self.dirty {
            self.save();
        }
    }
}
