use crate::{
    dsp::{Params, Processor},
    profile::Profile,
};
use std::{
    ffi::{c_char, c_void, CStr},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc,
    },
};
#[repr(C)]
#[derive(Clone, Copy)]
struct RawDevice {
    id: u32,
    uid: [c_char; 512],
    name: [c_char; 512],
    rate: f64,
}
#[derive(Clone, Debug)]
pub struct Device {
    pub id: u32,
    pub uid: String,
    pub name: String,
    pub rate: f64,
    pub supported: bool,
}
type Process =
    unsafe extern "C" fn(*mut c_void, *const f32, *const f32, u32, *mut f32, *mut f32, u32, u32);
unsafe extern "C" {
    fn me_devices(out: *mut RawDevice, capacity: i32) -> i32;
    fn me_supported(device: u32) -> i32;
    fn me_start(
        device: u32,
        process: Process,
        ctx: *mut c_void,
        rate: *mut f64,
        error: *mut c_char,
        len: usize,
    ) -> *mut c_void;
    fn me_valid(engine: *mut c_void) -> i32;
    fn me_stop(engine: *mut c_void) -> i32;
}
pub fn devices() -> Result<Vec<Device>, String> {
    let mut raw = [RawDevice {
        id: 0,
        uid: [0; 512],
        name: [0; 512],
        rate: 0.,
    }; 128];
    let n = unsafe { me_devices(raw.as_mut_ptr(), raw.len() as i32) };
    if n < 0 {
        return Err("Could not enumerate audio outputs".into());
    }
    Ok(raw[..n as usize]
        .iter()
        .map(|d| Device {
            id: d.id,
            uid: unsafe { CStr::from_ptr(d.uid.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
            name: unsafe { CStr::from_ptr(d.name.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
            rate: d.rate,
            supported: unsafe { me_supported(d.id) != 0 },
        })
        .collect())
}
#[derive(Default)]
pub struct Meters {
    pub peaks: [AtomicU32; 2],
    pub frames: AtomicU64,
}
struct Context {
    processor: Processor,
    updates: rtrb::Consumer<Params>,
    meters: Arc<Meters>,
    capture: crate::spectrum::Capture,
}
// Safety: HAL serializes this IOProc. The native bridge validates 32-bit float
// buffers, stride and frame bounds. The context outlives Stop + DestroyIOProc.
unsafe extern "C" fn process(
    ctx: *mut c_void,
    left: *const f32,
    right: *const f32,
    ins: u32,
    out_l: *mut f32,
    out_r: *mut f32,
    outs: u32,
    frames: u32,
) {
    let c = unsafe { &mut *ctx.cast::<Context>() };
    let mut latest = None;
    for _ in 0..8 {
        match c.updates.pop() {
            Ok(p) => latest = Some(p),
            Err(_) => break,
        }
    }
    if let Some(p) = latest {
        c.processor.update(p);
    }
    let analyze = c.capture.begin_buffer();
    let mut peaks = [0f32; 2];
    for i in 0..frames as usize {
        let input = unsafe { [*left.add(i * ins as usize), *right.add(i * ins as usize)] };
        let y = c.processor.frame(input);
        if analyze {
            c.capture.feed(y);
        }
        unsafe {
            *out_l.add(i * outs as usize) = y[0];
            *out_r.add(i * outs as usize) = y[1];
        }
        for ch in 0..2 {
            peaks[ch] = peaks[ch].max(y[ch].abs());
        }
    }
    for (ch, p) in peaks.iter().enumerate() {
        c.meters.peaks[ch].store(p.to_bits(), Ordering::Relaxed);
    }
    c.meters.frames.fetch_add(frames as u64, Ordering::Relaxed);
}
pub struct Engine {
    native: *mut c_void,
    context: *mut Context,
    producer: rtrb::Producer<Params>,
    pending: Option<Params>,
    pub meters: Arc<Meters>,
    pub spectrum: crate::spectrum::Spectrum,
    pub rate: f64,
    pub device_id: u32,
}
impl Engine {
    pub fn start(device: &Device, profile: &Profile) -> Result<Self, String> {
        profile.validate()?;
        if !device.supported || !device.rate.is_finite() || device.rate < 8000. {
            return Err("Choose a stereo PCM output; this layout is unsupported".into());
        }
        let params = Params::compile(profile, device.rate);
        let (producer, updates) = rtrb::RingBuffer::new(8);
        let meters = Arc::new(Meters::default());
        let (capture, spectrum) = crate::spectrum::channel();
        let context = Box::into_raw(Box::new(Context {
            processor: Processor::new(params, device.rate),
            updates,
            capture,
            meters: meters.clone(),
        }));
        let mut rate = device.rate;
        let mut error = [0 as c_char; 1024];
        let native = unsafe {
            me_start(
                device.id,
                process,
                context.cast(),
                &mut rate,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if native.is_null() {
            unsafe {
                drop(Box::from_raw(context));
            }
            return Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned());
        }
        Ok(Self {
            native,
            context,
            producer,
            pending: None,
            meters,
            spectrum,
            rate,
            device_id: device.id,
        })
    }
    pub fn update(&mut self, profile: &Profile) {
        self.pending = Some(Params::compile(profile, self.rate));
        self.flush();
    }
    pub fn flush(&mut self) {
        if let Some(p) = self.pending.take() {
            if let Err(rtrb::PushError::Full(p)) = self.producer.push(p) {
                self.pending = Some(p);
            }
        }
    }
    pub fn valid(&self) -> bool {
        unsafe { me_valid(self.native) != 0 }
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        if self.native.is_null() {
            return Ok(());
        }
        if unsafe { me_stop(self.native) } == 0 {
            return Err(
                "Core Audio could not finish cleanup. Quit MacEQ before starting another session."
                    .into(),
            );
        }
        self.native = std::ptr::null_mut();
        unsafe {
            drop(Box::from_raw(self.context));
        }
        self.context = std::ptr::null_mut();
        Ok(())
    }
    pub fn peaks(&self) -> [f32; 2] {
        self.meters
            .peaks
            .each_ref()
            .map(|p| f32::from_bits(p.load(Ordering::Relaxed)))
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("MacEQ: {error}; callback storage retained until process exit");
        }
    }
}
// Explicitly pinned UID: default-output changes have no bearing on this policy.
pub fn target<'a>(uid: &str, devices: &'a [Device]) -> Option<&'a Device> {
    devices.iter().find(|d| d.uid == uid)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pin_does_not_fall_back() {
        let ds = vec![Device {
            id: 1,
            uid: "other".into(),
            name: "Other".into(),
            rate: 48000.,
            supported: true,
        }];
        assert!(target("missing", &ds).is_none());
        assert_eq!(target("other", &ds).unwrap().id, 1);
    }
    #[test]
    fn callback_maps_interleaved_to_planar_without_overwriting_guards() {
        let mut p = Profile::default();
        for channel in &mut p.channels {
            channel.preamp = 0.;
        }
        let params = Params::compile(&p, 48000.);
        let (_producer, updates) = rtrb::RingBuffer::new(8);
        let meters = Arc::new(Meters::default());
        let capture = crate::spectrum::channel().0;
        let mut ctx = Context {
            processor: Processor::new(params, 48000.),
            updates,
            capture,
            meters: meters.clone(),
        };
        let input = [0.1f32, 0.2, 0.3, 0.4];
        let mut left = [-9f32; 4];
        let mut right = [-9f32; 4];
        unsafe {
            process(
                (&mut ctx as *mut Context).cast(),
                input.as_ptr(),
                input.as_ptr().add(1),
                2,
                left.as_mut_ptr().add(1),
                right.as_mut_ptr().add(1),
                1,
                2,
            );
        }
        assert_eq!(left, [-9., 0.1, 0.3, -9.]);
        assert_eq!(right, [-9., 0.2, 0.4, -9.]);
        assert_eq!(meters.frames.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn callback_maps_planar_to_interleaved() {
        let mut p = Profile::default();
        for channel in &mut p.channels {
            channel.preamp = 0.;
        }
        let params = Params::compile(&p, 48000.);
        let (_producer, updates) = rtrb::RingBuffer::new(8);
        let capture = crate::spectrum::channel().0;
        let mut ctx = Context {
            processor: Processor::new(params, 48000.),
            updates,
            capture,
            meters: Arc::default(),
        };
        let left = [0.1f32, 0.3];
        let right = [0.2f32, 0.4];
        let mut out = [0f32; 4];
        unsafe {
            process(
                (&mut ctx as *mut Context).cast(),
                left.as_ptr(),
                right.as_ptr(),
                1,
                out.as_mut_ptr(),
                out.as_mut_ptr().add(1),
                2,
                2,
            );
        }
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4]);
    }
    #[test]
    fn spectrum_receives_processed_audio_without_blocking_when_full() {
        use crate::spectrum::FFT_SIZE;
        use std::f32::consts::PI;
        let p = Profile::default();
        let params = Params::compile(&p, 48000.);
        let (_producer, updates) = rtrb::RingBuffer::new(8);
        let (capture, mut spectrum) = crate::spectrum::channel();
        let mut ctx = Context {
            processor: Processor::new(params, 48000.),
            updates,
            capture,
            meters: Arc::default(),
        };
        let left: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (2. * PI * 64. * i as f32 / FFT_SIZE as f32).sin() * 0.5)
            .collect();
        let right = vec![0f32; FFT_SIZE];
        let mut ol = vec![0f32; FFT_SIZE];
        let mut or = vec![0f32; FFT_SIZE];
        // Three callbacks overflow the display queue. Playback must still advance.
        for _ in 0..3 {
            unsafe {
                process(
                    (&mut ctx as *mut Context).cast(),
                    left.as_ptr(),
                    right.as_ptr(),
                    1,
                    ol.as_mut_ptr(),
                    or.as_mut_ptr(),
                    1,
                    FFT_SIZE as u32,
                );
            }
        }
        assert_eq!(
            ctx.meters.frames.load(Ordering::Relaxed),
            3 * FFT_SIZE as u64
        );
        assert!((ol[16] - left[16] * 10f32.powf(-6. / 20.)).abs() < 1e-5);
        assert!(or.iter().all(|x| *x == 0.));
        spectrum.poll();
        assert!((spectrum.levels(0).unwrap()[64] - (-12.0206)).abs() < 0.05);
        assert!(spectrum.levels(1).unwrap().iter().all(|db| *db <= -99.));
        let expected = ol.clone();
        spectrum.set_active(false);
        unsafe {
            process(
                (&mut ctx as *mut Context).cast(),
                left.as_ptr(),
                right.as_ptr(),
                1,
                ol.as_mut_ptr(),
                or.as_mut_ptr(),
                1,
                FFT_SIZE as u32,
            );
        }
        spectrum.poll();
        assert!(spectrum.levels(0).is_none());
        assert_eq!(
            ol, expected,
            "Pausing visualization must not change EQ audio"
        );
    }
}
