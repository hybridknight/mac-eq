use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub const MAX_BANDS: usize = 10;
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Filter {
    Peak,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
}
impl Filter {
    pub const ALL: [Self; 5] = [
        Self::Peak,
        Self::LowShelf,
        Self::HighShelf,
        Self::LowPass,
        Self::HighPass,
    ];
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Band {
    pub enabled: bool,
    pub kind: Filter,
    pub hz: f64,
    pub db: f64,
    pub q: f64,
}
impl Default for Band {
    fn default() -> Self {
        Self {
            enabled: true,
            kind: Filter::Peak,
            hz: 1000.,
            db: 0.,
            q: 0.707,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub preamp: f64,
    pub bands: Vec<Band>,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            preamp: 0.,
            bands: [
                31.5, 63., 125., 250., 500., 1000., 2000., 4000., 8000., 16000.,
            ]
            .into_iter()
            .map(|hz| Band {
                hz,
                q: 1.4,
                ..Band::default()
            })
            .collect(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub linked: bool,
    pub channels: [Channel; 2],
    pub bypass: bool,
    pub limiter: bool,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            linked: true,
            channels: [Channel::default(), Channel::default()],
            bypass: false,
            limiter: true,
        }
    }
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        for c in &self.channels {
            if !c.preamp.is_finite()
                || !(-60. ..=12.).contains(&c.preamp)
                || c.bands.len() > MAX_BANDS
            {
                return Err("Invalid preamp or band count".into());
            }
            for b in &c.bands {
                if !b.hz.is_finite()
                    || !(20. ..=20000.).contains(&b.hz)
                    || !b.db.is_finite()
                    || !(-24. ..=24.).contains(&b.db)
                    || !b.q.is_finite()
                    || !(0.1..=20.).contains(&b.q)
                {
                    return Err("Invalid EQ band".into());
                }
            }
        }
        if self.linked
            && serde_json::to_string(&self.channels[0]).ok()
                != serde_json::to_string(&self.channels[1]).ok()
        {
            return Err("Linked channels must match".into());
        }
        Ok(())
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub selected: String,
    #[serde(default)]
    pub resume_on_connect: bool,
    // None migrates the original single selected-device preference without
    // accidentally enabling any other previously saved profile.
    #[serde(default)]
    pub enabled_devices: Option<BTreeSet<String>>,
    #[serde(default)]
    pub device_names: BTreeMap<String, String>,
    pub devices: BTreeMap<String, Profile>,
    pub presets: BTreeMap<String, Profile>,
}
impl Settings {
    pub fn is_enabled(&self, uid: &str) -> bool {
        if uid.is_empty() {
            return false;
        }
        match &self.enabled_devices {
            Some(devices) => devices.contains(uid),
            None => self.resume_on_connect && self.selected == uid,
        }
    }
    pub fn enabled_uids(&self) -> Vec<String> {
        match &self.enabled_devices {
            Some(devices) => devices.iter().cloned().collect(),
            None if self.resume_on_connect && !self.selected.is_empty() => {
                vec![self.selected.clone()]
            }
            None => Vec::new(),
        }
    }
    pub fn migrate_enablement(&mut self) {
        if self.enabled_devices.is_none() {
            self.enabled_devices = Some(self.enabled_uids().into_iter().collect());
        }
    }
    pub fn set_enabled(&mut self, uid: &str, enabled: bool) {
        if uid.is_empty() {
            return;
        }
        self.migrate_enablement();
        let devices = self.enabled_devices.as_mut().unwrap();
        if enabled {
            devices.insert(uid.into());
        } else {
            devices.remove(uid);
        }
        self.resume_on_connect = self.is_enabled(&self.selected);
    }

    pub fn path() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join("Library/Application Support/MacEQ/settings.json")
    }
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self {
                version: 1,
                ..Self::default()
            });
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let value: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Settings unreadable; original preserved: {e}"))?;
        if value.version != 1 {
            return Err("Unsupported settings version; original preserved".into());
        }
        for profile in value.devices.values().chain(value.presets.values()) {
            profile.validate()?;
        }
        Ok(value)
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        for profile in self.devices.values().chain(self.presets.values()) {
            profile.validate()?;
        }
        let parent = path.parent().ok_or("Invalid settings path")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut f = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
        f.write_all(&data)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_to_ten_flat_bands_per_channel() {
        let profile = Profile::default();
        assert!(profile.validate().is_ok());
        for channel in &profile.channels {
            assert_eq!(channel.preamp, 0.);
            assert_eq!(channel.bands.len(), 10);
            assert_eq!(
                channel.bands.iter().map(|band| band.hz).collect::<Vec<_>>(),
                vec![31.5, 63., 125., 250., 500., 1000., 2000., 4000., 8000., 16000.]
            );
            assert!(channel.bands.iter().all(|band| {
                band.enabled && band.kind == Filter::Peak && band.db == 0. && band.q == 1.4
            }));
        }
    }
    #[test]
    fn saved_custom_band_layout_is_preserved() {
        let mut profile = Profile::default();
        for channel in &mut profile.channels {
            channel.bands = vec![Band {
                hz: 750.,
                db: -3.,
                ..Band::default()
            }];
        }
        let saved = serde_json::to_string(&profile).unwrap();
        let loaded: Profile = serde_json::from_str(&saved).unwrap();
        assert!(loaded.validate().is_ok());
        assert_eq!(serde_json::to_string(&loaded).unwrap(), saved);
    }
    #[test]
    fn validates_and_roundtrips() {
        let p = Profile::default();
        assert!(p.validate().is_ok());
        let p: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p.channels[0].preamp, 0.);
    }
    #[test]
    fn rejects_bad_profiles() {
        let mut p = Profile::default();
        p.channels[0].bands[0].hz = f64::NAN;
        assert!(p.validate().is_err());
    }
    #[test]
    fn preserves_future_settings() {
        let path = std::env::temp_dir().join(format!("maceq-future-{}.json", std::process::id()));
        let data = r#"{"version":99,"selected":"","devices":{},"presets":{}}"#;
        std::fs::write(&path, data).unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), data);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn atomic_settings_roundtrip_preserves_device_profiles() {
        let dir = std::env::temp_dir().join(format!("maceq-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut settings = Settings {
            version: 1,
            selected: "dac-uid".into(),
            ..Settings::default()
        };
        let mut p = Profile {
            linked: false,
            ..Profile::default()
        };
        p.channels[1].preamp = -12.;
        settings.devices.insert(settings.selected.clone(), p);
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.selected, "dac-uid");
        assert_eq!(loaded.devices["dac-uid"].channels[1].preamp, -12.);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
