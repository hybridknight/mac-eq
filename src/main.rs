mod audio;
mod chart;
mod dsp;
#[cfg(feature = "memory-profile")]
mod memory_profile;
mod platform;
mod profile;
mod spectrum;
mod theme;
mod ui;
use eframe::egui;
use profile::{Profile, Settings};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

struct DeviceSession {
    engine: Option<audio::Engine>,
    retry: Instant,
    last_frames: u64,
    heartbeat: Instant,
    status: String,
}
impl Default for DeviceSession {
    fn default() -> Self {
        Self {
            engine: None,
            retry: Instant::now(),
            last_frames: 0,
            heartbeat: Instant::now(),
            status: "Waiting for device".into(),
        }
    }
}
impl DeviceSession {
    fn shutdown(&mut self) -> Result<(), String> {
        if let Some(engine) = &mut self.engine {
            engine.shutdown()?;
        }
        self.engine = None;
        Ok(())
    }
}
struct MacEQ {
    settings: Settings,
    profile: Profile,
    devices: Vec<audio::Device>,
    sessions: std::collections::BTreeMap<String, DeviceSession>,
    cleanup_failed: bool,
    error: String,
    save_error: String,
    can_save: bool,
    channel: usize,
    preset_name: String,
    last_poll: Instant,
    dirty: bool,
    last_edit: Instant,
    platform: Option<platform::Platform>,
    app_icon: Option<egui::TextureHandle>,
    login_status: i32,
    login_error: String,
    quitting: bool,
    editor_visible: bool,
    #[cfg(feature = "memory-profile")]
    memory_probe: memory_profile::Probe,
}
impl MacEQ {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::configure(&cc.egui_ctx);
        let (settings, error, can_save) = if cfg!(feature = "memory-profile") {
            let mut settings = Settings {
                selected: "memory-test".into(),
                ..Settings::default()
            };
            settings
                .device_names
                .insert("memory-test".into(), "Benchmark output".into());
            (settings, String::new(), false)
        } else {
            match Settings::load(&Settings::path()) {
                Ok(s) => (s, String::new(), true),
                Err(e) => (
                    Settings {
                        version: 1,
                        ..Settings::default()
                    },
                    e,
                    false,
                ),
            }
        };
        let mut app = Self::from_settings(
            settings,
            if cfg!(feature = "memory-profile") {
                Vec::new()
            } else {
                audio::devices().unwrap_or_default()
            },
            error,
            can_save,
        );
        if let Ok(icon) =
            eframe::icon_data::from_png_bytes(include_bytes!("../bundle/generated/AppIcon256.png"))
        {
            app.app_icon = Some(cc.egui_ctx.load_texture(
                "MacEQ artwork",
                egui::ColorImage::from(icon),
                egui::TextureOptions::LINEAR,
            ));
        }
        let mut integration = platform::Platform::new(cc.egui_ctx.clone());
        let background = platform::start_in_background();
        #[cfg(feature = "memory-profile")]
        if background {
            app.memory_probe.start_hidden();
        }
        app.editor_visible = !background;
        if let Err(error) = integration.set_editor_visible(!background) {
            app.error = error;
        }
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(!background));
        if !background {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        app.platform = Some(integration);
        app.login_status = platform::login_status();
        app
    }
    fn from_settings(
        mut settings: Settings,
        devices: Vec<audio::Device>,
        error: String,
        can_save: bool,
    ) -> Self {
        settings.migrate_enablement();
        let profile = settings
            .devices
            .get(&settings.selected)
            .cloned()
            .unwrap_or_default();
        Self {
            settings,
            profile,
            devices,
            sessions: Default::default(),
            cleanup_failed: false,
            error,
            save_error: String::new(),
            can_save,
            channel: 0,
            preset_name: String::new(),
            last_poll: Instant::now() - Duration::from_secs(2),
            dirty: false,
            last_edit: Instant::now(),
            platform: None,
            app_icon: None,
            login_status: 0,
            login_error: String::new(),
            quitting: false,
            editor_visible: true,
            #[cfg(feature = "memory-profile")]
            memory_probe: Default::default(),
        }
    }
    fn select_device(&mut self, uid: String) {
        if uid == self.settings.selected {
            return;
        }
        // Selecting an editor never starts/stops or re-routes any audio session.
        self.settings.selected = uid;
        self.profile = self
            .settings
            .devices
            .get(&self.settings.selected)
            .cloned()
            .unwrap_or_default();
        self.settings
            .devices
            .insert(self.settings.selected.clone(), self.profile.clone());
        self.dirty = true;
        self.save();
    }
    fn set_device_enabled(&mut self, uid: &str, enabled: bool) {
        if uid.is_empty() {
            return;
        }
        self.settings.set_enabled(uid, enabled);
        self.settings.devices.entry(uid.into()).or_default();
        self.dirty = true;
        self.save();
        if !enabled {
            if let Some(session) = self.sessions.get_mut(uid) {
                if let Err(error) = session.shutdown() {
                    self.error = error;
                    self.cleanup_failed = true;
                    return;
                }
            }
            self.sessions.remove(uid);
        } else {
            self.sessions.entry(uid.into()).or_default().retry = Instant::now();
        }
        self.last_poll = Instant::now() - Duration::from_secs(2);
    }
    fn enable(&mut self) {
        let uid = self.settings.selected.clone();
        self.set_device_enabled(&uid, true);
    }
    fn stop(&mut self) {
        let uid = self.settings.selected.clone();
        self.set_device_enabled(&uid, false);
    }
    fn pending_device(&self, uid: &str) -> Option<&audio::Device> {
        if !self.settings.is_enabled(uid) || self.cleanup_failed {
            return None;
        }
        if self
            .sessions
            .get(uid)
            .is_some_and(|s| s.engine.is_some() || Instant::now() < s.retry)
        {
            return None;
        }
        audio::target(uid, &self.devices).filter(|d| d.supported)
    }
    fn selected_status(&self) -> &str {
        if !self.settings.is_enabled(&self.settings.selected) {
            "EQ disabled · device untouched"
        } else {
            self.sessions
                .get(&self.settings.selected)
                .map_or("Waiting for device", |s| s.status.as_str())
        }
    }
    fn changed(&mut self) {
        if self.profile.linked {
            self.profile.channels[1 - self.channel] = self.profile.channels[self.channel].clone();
        }
        self.settings
            .devices
            .insert(self.settings.selected.clone(), self.profile.clone());
        self.dirty = true;
        self.last_edit = Instant::now();
        if let Some(engine) = self
            .sessions
            .get_mut(&self.settings.selected)
            .and_then(|s| s.engine.as_mut())
        {
            engine.update(&self.profile);
        }
    }
    fn save(&mut self) {
        self.last_edit = Instant::now();
        if !self.can_save {
            self.save_error="Settings could not be loaded safely; original file preserved. Back up and move it before restarting to enable saving.".into();
            return;
        }
        match self.settings.save(&Settings::path()) {
            Ok(()) => {
                self.dirty = false;
                self.save_error.clear();
            }
            Err(e) => self.save_error = e,
        }
    }
    fn close_engines(&mut self) {
        for session in self.sessions.values_mut() {
            if let Err(error) = session.shutdown() {
                self.error = error;
                self.cleanup_failed = true;
            }
        }
    }
    fn tick(&mut self, editor_active: bool) {
        if cfg!(feature = "memory-profile") {
            return;
        }
        if self.dirty && self.last_edit.elapsed() > Duration::from_millis(600) {
            self.save();
        }
        if self.cleanup_failed {
            return;
        }
        for (uid, session) in &mut self.sessions {
            if let Some(engine) = session.engine.as_mut() {
                engine.flush();
                let analyze = editor_active && uid == &self.settings.selected;
                engine.spectrum.set_active(analyze);
                if analyze {
                    engine.spectrum.poll();
                }
                let frames = engine.meters.frames.load(Ordering::Relaxed);
                if frames != session.last_frames {
                    session.last_frames = frames;
                    session.heartbeat = Instant::now();
                    session.status = if engine.peaks().iter().all(|p| *p < 1e-7) {
                        "Active · silent input"
                    } else {
                        "Processing audio"
                    }
                    .into();
                }
            }
        }
        if self.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_poll = Instant::now();
        match audio::devices() {
            Ok(ds) => self.devices = ds,
            Err(e) => {
                self.error = e;
                return;
            }
        }
        if self.platform.is_some() {
            self.login_status = platform::login_status();
        }
        for device in &self.devices {
            if self.settings.device_names.get(&device.uid) != Some(&device.name) {
                self.settings
                    .device_names
                    .insert(device.uid.clone(), device.name.clone());
                self.dirty = true;
            }
        }
        // Iterate only explicit opt-ins. Disabled devices never create taps,
        // aggregates or IOProcs, even if selected in the settings editor.
        for uid in self.settings.enabled_uids() {
            let device = audio::target(&uid, &self.devices).cloned();
            let session = self.sessions.entry(uid.clone()).or_default();
            let valid = session.engine.as_ref().is_none_or(|engine| {
                device.as_ref().is_some_and(|d| {
                    d.id == engine.device_id
                        && d.rate == engine.rate
                        && d.supported
                        && engine.valid()
                })
            });
            let stalled =
                session.engine.is_some() && session.heartbeat.elapsed() > Duration::from_secs(5);
            if !valid || stalled {
                if let Err(error) = session.shutdown() {
                    self.error = error;
                    self.cleanup_failed = true;
                    return;
                }
                session.retry = Instant::now() + Duration::from_secs(if stalled { 5 } else { 1 });
                session.status = if stalled {
                    "No audio buffers · retrying"
                } else {
                    "Waiting for device"
                }
                .into();
                if self.dirty {
                    self.save();
                }
            }
            if self.pending_device(&uid).is_none() {
                continue;
            }
            let Some(device) = device else {
                continue;
            };
            let profile = self.settings.devices.get(&uid).cloned().unwrap_or_default();
            let session = self.sessions.get_mut(&uid).unwrap();
            session.retry = Instant::now() + Duration::from_secs(5);
            match audio::Engine::start(&device, &profile) {
                Ok(engine) => {
                    session.engine = Some(engine);
                    session.heartbeat = Instant::now();
                    session.last_frames = 0;
                    session.status = "Starting audio…".into();
                }
                Err(error) => {
                    session.status = format!("{error} · retrying");
                }
            }
        }
    }
    fn set_login(&mut self, enabled: bool) {
        self.login_error = match platform::set_login(enabled) {
            Ok(()) => String::new(),
            Err(e) => e,
        };
        self.login_status = platform::login_status();
    }
    fn platform_events(&mut self, ctx: &egui::Context) {
        if let Some(platform) = &mut self.platform {
            let (events, uid) = platform.poll();
            if events & 1 != 0 {
                self.editor_visible = true;
                if let Err(error) = platform.set_editor_visible(true) {
                    self.error = error;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if events & 2 != 0 {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if events & 8 != 0 {
                self.set_login(!matches!(self.login_status, 1 | 2));
            }
            if events & 16 != 0 {
                let enabled = !self.settings.is_enabled(&uid);
                self.set_device_enabled(&uid, enabled);
            }
        }
        if ctx.input(|input| input.viewport().close_requested())
            && !self.quitting
            && self.platform.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.editor_visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            if let Some(platform) = &mut self.platform {
                if let Err(error) = platform.set_editor_visible(false) {
                    self.error = error;
                }
            }
            if self.dirty {
                self.save();
            }
        }
        let active = self
            .sessions
            .values()
            .filter(|s| s.engine.is_some())
            .count();
        if let Some(platform) = &mut self.platform {
            platform.sync(&self.settings, &self.devices, active, self.login_status);
        }
    }
}
fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--list-devices") {
        match audio::devices() {
            Ok(ds) => {
                for d in ds {
                    println!(
                        "{}\n  UID: {}\n  {:.0} Hz · {}",
                        d.name,
                        d.uid,
                        d.rate,
                        if d.supported {
                            "stereo PCM"
                        } else {
                            "unsupported layout"
                        }
                    );
                }
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    // --background exercises the same window policy without changing Login Items.
    platform::prepare_launch(std::env::args().any(|arg| arg == "--background"));
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../bundle/generated/AppIcon256.png"))
            .expect("Invalid bundled MacEQ artwork");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MacEQ")
            .with_active(false)
            .with_visible(false)
            .with_icon(std::sync::Arc::new(icon))
            .with_inner_size([1120., 860.])
            .with_min_inner_size([1000., 740.]),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder
                .with_activation_policy(ActivationPolicy::Accessory)
                .with_activate_ignoring_other_apps(false);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "MacEQ",
        options,
        Box::new(|cc| Ok(Box::new(MacEQ::new(cc)))),
    )
}

#[cfg(test)]
mod reconnect_tests {
    use super::*;
    fn device(uid: &str, id: u32) -> audio::Device {
        audio::Device {
            id,
            uid: uid.into(),
            name: "USB DAC".into(),
            rate: 48000.,
            supported: true,
        }
    }
    #[test]
    fn saved_channels_restore_after_relaunch_and_replug_with_new_object_id() {
        let dir = std::env::temp_dir().join(format!("maceq-reconnect-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut settings = Settings {
            version: 1,
            selected: "dac-stable-uid".into(),
            resume_on_connect: true,
            ..Settings::default()
        };
        let mut profile = Profile {
            linked: false,
            ..Profile::default()
        };
        profile.channels[0].preamp = -9.;
        profile.channels[0].bands[0].db = -7.;
        profile.channels[1].preamp = -3.;
        profile.channels[1].bands[0].hz = 2300.;
        profile.bypass = true;
        settings.devices.insert(settings.selected.clone(), profile);
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        let mut app = MacEQ::from_settings(
            loaded,
            vec![device("unrelated-uid", 1)],
            String::new(),
            false,
        );
        assert!(app.settings.is_enabled(&app.settings.selected));
        assert!(app.pending_device(&app.settings.selected).is_none());
        // The runtime AudioObjectID changes after unplugging. Only UID identifies the profile.
        app.devices.push(device("dac-stable-uid", 99));
        assert_eq!(app.pending_device(&app.settings.selected).unwrap().id, 99);
        assert_eq!(app.profile.channels[0].preamp, -9.);
        assert_eq!(app.profile.channels[0].bands[0].db, -7.);
        assert_eq!(app.profile.channels[1].preamp, -3.);
        assert_eq!(app.profile.channels[1].bands[0].hz, 2300.);
        assert!(app.profile.bypass);
        app.devices.retain(|d| d.uid != "dac-stable-uid");
        assert!(app.pending_device(&app.settings.selected).is_none());
        assert!(app.settings.is_enabled(&app.settings.selected));
        app.devices.push(device("dac-stable-uid", 101));
        assert_eq!(app.pending_device(&app.settings.selected).unwrap().id, 101);
        // Explicit Stop cancels automatic restoration and persists across another relaunch.
        app.stop();
        app.settings.save(&path).unwrap();
        let stopped = MacEQ::from_settings(
            Settings::load(&path).unwrap(),
            vec![device("dac-stable-uid", 102)],
            String::new(),
            false,
        );
        assert!(!stopped.settings.is_enabled(&stopped.settings.selected));
        assert!(stopped.pending_device(&stopped.settings.selected).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn existing_settings_without_resume_field_stay_stopped() {
        let settings: Settings =
            serde_json::from_str(r#"{"version":1,"selected":"dac","devices":{},"presets":{}}"#)
                .unwrap();
        let app = MacEQ::from_settings(settings, vec![device("dac", 1)], String::new(), false);
        assert!(!app.settings.is_enabled(&app.settings.selected));
        assert!(app.pending_device(&app.settings.selected).is_none());
    }
    #[test]
    fn reconnect_waits_for_retry_deadline_and_supported_format() {
        let settings = Settings {
            version: 1,
            selected: "dac".into(),
            resume_on_connect: true,
            ..Settings::default()
        };
        let mut app = MacEQ::from_settings(settings, vec![device("dac", 1)], String::new(), false);
        app.sessions.entry("dac".into()).or_default().retry =
            Instant::now() + Duration::from_secs(5);
        assert!(app.pending_device(&app.settings.selected).is_none());
        app.sessions.get_mut("dac").unwrap().retry = Instant::now();
        app.devices[0].supported = false;
        assert!(app.pending_device(&app.settings.selected).is_none());
        app.devices[0].supported = true;
        assert!(app.pending_device(&app.settings.selected).is_some());
    }
    #[test]
    fn editor_selection_never_enables_or_retargets_disabled_device() {
        let mut settings = Settings {
            version: 1,
            selected: "enabled".into(),
            ..Settings::default()
        };
        settings.set_enabled("enabled", true);
        let mut app = MacEQ::from_settings(
            settings,
            vec![device("enabled", 1), device("disabled", 2)],
            String::new(),
            false,
        );
        assert!(app.pending_device("enabled").is_some());
        assert!(app.pending_device("disabled").is_none());
        app.select_device("disabled".into());
        assert_eq!(app.settings.selected, "disabled");
        assert!(app.pending_device("enabled").is_some());
        assert!(app.pending_device("disabled").is_none());
        // Editing the disabled output persists only its profile, not enablement.
        app.profile.channels[0].preamp = -12.;
        app.changed();
        assert!(app.pending_device("disabled").is_none());
        assert_eq!(app.settings.devices["disabled"].channels[0].preamp, -12.);
    }
    #[test]
    fn enablement_is_independent_and_disabling_one_preserves_other_session() {
        let mut settings = Settings {
            version: 1,
            selected: "one".into(),
            ..Settings::default()
        };
        settings.set_enabled("one", true);
        settings.set_enabled("two", true);
        let mut app = MacEQ::from_settings(
            settings,
            vec![device("one", 1), device("two", 2)],
            String::new(),
            false,
        );
        app.sessions.insert(
            "two".into(),
            DeviceSession {
                status: "Unchanged session".into(),
                ..Default::default()
            },
        );
        app.set_device_enabled("one", false);
        assert!(app.pending_device("one").is_none());
        assert!(app.pending_device("two").is_some());
        assert_eq!(app.sessions["two"].status, "Unchanged session");
        // A profile may be saved for a disabled device without opting it into capture.
        app.settings
            .devices
            .insert("three".into(), Profile::default());
        assert!(!app.settings.is_enabled("three"));
    }
    #[test]
    fn device_flags_persist_even_when_editor_is_on_disabled_output() {
        let dir = std::env::temp_dir().join(format!("maceq-device-flags-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut settings = Settings {
            version: 1,
            selected: "off".into(),
            ..Settings::default()
        };
        settings.set_enabled("on", true);
        settings.set_enabled("off", false);
        settings.devices.insert("off".into(), Profile::default());
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert!(loaded.is_enabled("on"));
        assert!(!loaded.is_enabled("off"));
        assert_eq!(loaded.selected, "off");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn legacy_enablement_migrates_only_previously_selected_device() {
        let mut settings = Settings {
            version: 1,
            selected: "old".into(),
            resume_on_connect: true,
            ..Settings::default()
        };
        settings.devices.insert("other".into(), Profile::default());
        settings.set_enabled("other", false);
        assert_eq!(settings.enabled_uids(), vec!["old"]);
        settings.selected = "other".into();
        assert!(!settings.is_enabled("other"));
        assert!(settings.is_enabled("old"));
    }
    #[test]
    fn legacy_selected_device_stays_enabled_when_editing_another_output() {
        let settings = Settings {
            version: 1,
            selected: "old".into(),
            resume_on_connect: true,
            ..Settings::default()
        };
        let mut app = MacEQ::from_settings(
            settings,
            vec![device("old", 1), device("new", 2)],
            String::new(),
            false,
        );
        app.select_device("new".into());
        assert!(app.settings.is_enabled("old"));
        assert!(!app.settings.is_enabled("new"));
    }
}
