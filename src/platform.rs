use crate::{audio::Device, profile::Settings};
use std::ffi::{c_char, c_void, CStr, CString};
use std::time::{Duration, Instant};
unsafe extern "C" {
    fn me_platform_create(
        wake: unsafe extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> *mut c_void;
    fn me_platform_poll(handle: *mut c_void, uid: *mut c_char, length: usize) -> u32;
    fn me_platform_sync(
        handle: *mut c_void,
        json: *const c_char,
        active: i32,
        enabled: i32,
        login: i32,
    );
    fn me_platform_destroy(handle: *mut c_void);
    fn me_prepare_launch(background: i32);
    fn me_start_in_background() -> i32;
    fn me_set_editor_visible(handle: *mut c_void, visible: i32) -> i32;
    fn me_login_status() -> i32;
    fn me_set_login(enabled: i32, error: *mut c_char, length: usize) -> i32;
    fn me_open_login_settings();
}
pub struct Platform {
    handle: *mut c_void,
    context: *mut eframe::egui::Context,
    last_json: String,
    last_state: (usize, usize, i32),
    last_sync: Instant,
}
unsafe extern "C" fn wake(context: *mut c_void) {
    unsafe { &*context.cast::<eframe::egui::Context>() }.request_repaint();
}
impl Platform {
    pub fn new(ctx: eframe::egui::Context) -> Self {
        let context = Box::into_raw(Box::new(ctx));
        let handle = unsafe { me_platform_create(wake, context.cast()) };
        Self {
            handle,
            context,
            last_json: String::new(),
            last_state: (usize::MAX, usize::MAX, -1),
            last_sync: Instant::now() - Duration::from_secs(1),
        }
    }
    pub fn set_editor_visible(&mut self, visible: bool) -> Result<(), String> {
        if unsafe { me_set_editor_visible(self.handle, visible as i32) } != 0 {
            Ok(())
        } else {
            Err(if visible {
                "macOS could not restore MacEQ’s Dock icon"
            } else {
                "macOS could not hide MacEQ from the Dock; EQ remains running"
            }
            .into())
        }
    }
    pub fn poll(&mut self) -> (u32, String) {
        let mut uid = [0 as c_char; 1024];
        let flags = unsafe { me_platform_poll(self.handle, uid.as_mut_ptr(), uid.len()) };
        (
            flags,
            unsafe { CStr::from_ptr(uid.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        )
    }
    pub fn sync(&mut self, settings: &Settings, devices: &[Device], active: usize, login: i32) {
        if self.last_sync.elapsed() < Duration::from_millis(250) {
            return;
        }
        self.last_sync = Instant::now();
        let rows = device_rows(settings, devices);
        let json = serde_json::to_string(&rows).unwrap_or_default();
        let enabled = settings.enabled_uids().len();
        let changed = json != self.last_json;
        if !changed && self.last_state == (active, enabled, login) {
            return;
        }
        let cjson = CString::new(json.as_str()).unwrap();
        unsafe {
            me_platform_sync(
                self.handle,
                if changed {
                    cjson.as_ptr()
                } else {
                    std::ptr::null()
                },
                active as i32,
                enabled as i32,
                login,
            );
        }
        self.last_json = json;
        self.last_state = (active, enabled, login);
    }
}
impl Drop for Platform {
    fn drop(&mut self) {
        unsafe {
            me_platform_destroy(self.handle);
            drop(Box::from_raw(self.context));
        }
    }
}
pub fn device_rows(settings: &Settings, devices: &[Device]) -> Vec<serde_json::Value> {
    let mut names = settings.device_names.clone();
    for device in devices {
        names.insert(device.uid.clone(), device.name.clone());
    }
    for uid in settings.enabled_uids() {
        names.entry(uid).or_insert_with(|| "Saved output".into());
    }
    for uid in settings.devices.keys() {
        names.entry(uid.clone()).or_insert_with(|| uid.clone());
    }
    names.into_iter().filter(|(uid,_)|!uid.is_empty()).map(|(uid,name)|serde_json::json!({"enabled":settings.is_enabled(&uid),"connected":devices.iter().any(|d|d.uid==uid),"uid":uid,"name":name})).collect()
}
pub fn login_status() -> i32 {
    unsafe { me_login_status() }
}
pub fn set_login(enabled: bool) -> Result<(), String> {
    let mut error = [0 as c_char; 1024];
    if unsafe { me_set_login(enabled as i32, error.as_mut_ptr(), error.len()) } != 0 {
        Ok(())
    } else {
        Err(unsafe { CStr::from_ptr(error.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }
}
pub fn open_login_settings() {
    unsafe {
        me_open_login_settings();
    }
}

// Called before the UI event loop, not from a later frame where the launch
// Apple Event would already have been consumed.
pub fn prepare_launch(background: bool) {
    unsafe {
        me_prepare_launch(background as i32);
    }
}
pub fn start_in_background() -> bool {
    unsafe { me_start_in_background() != 0 }
}
