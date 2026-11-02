//! Display-only analysis. The HAL thread copies bounded chunks; FFT work runs on
//! the UI thread. A full queue drops visualization data, never playback audio.
use std::{
    f32::consts::PI,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub const FFT_SIZE: usize = 4096;
pub const BINS: usize = FFT_SIZE / 2 + 1;
const CHUNK: usize = 256;
const QUEUE: usize = 32;
const FLOOR: f32 = -100.;
#[derive(Clone, Copy)]
struct Block {
    generation: u64,
    sequence: u64,
    samples: [[f32; 2]; CHUNK],
}
pub struct Capture {
    state: Arc<AtomicU64>,
    producer: rtrb::Producer<Block>,
    block: Block,
    used: usize,
}
pub struct Spectrum {
    state: Arc<AtomicU64>,
    consumer: rtrb::Consumer<Block>,
    history: [[f32; FFT_SIZE]; 2],
    cursor: usize,
    filled: usize,
    sequence: Option<u64>,
    levels: [[f32; BINS]; 2],
    real: [f32; FFT_SIZE],
    imag: [f32; FFT_SIZE],
    window: [f32; FFT_SIZE],
    scale: f32,
    last_data: Instant,
}
pub fn channel() -> (Capture, Spectrum) {
    // Low bit enables analysis; higher bits identify a fresh display session.
    let state = Arc::new(AtomicU64::new(1));
    let (producer, consumer) = rtrb::RingBuffer::new(QUEUE);
    let window = std::array::from_fn(|i| 0.5 - 0.5 * (2. * PI * i as f32 / FFT_SIZE as f32).cos());
    let scale = 2. / window.iter().sum::<f32>();
    (
        Capture {
            state: state.clone(),
            producer,
            block: Block {
                generation: 1,
                sequence: 0,
                samples: [[0.; 2]; CHUNK],
            },
            used: 0,
        },
        Spectrum {
            state,
            consumer,
            history: [[0.; FFT_SIZE]; 2],
            cursor: 0,
            filled: 0,
            sequence: None,
            levels: [[FLOOR; BINS]; 2],
            real: [0.; FFT_SIZE],
            imag: [0.; FFT_SIZE],
            window,
            scale,
            last_data: Instant::now(),
        },
    )
}
impl Capture {
    /// Read once per audio callback, never lock or allocate on the HAL thread.
    pub fn begin_buffer(&mut self) -> bool {
        let state = self.state.load(Ordering::Relaxed);
        if self.block.generation != state {
            self.used = 0;
            self.block.generation = state;
        }
        state & 1 != 0
    }
    pub fn feed(&mut self, frame: [f32; 2]) {
        self.block.samples[self.used] = frame;
        self.used += 1;
        if self.used == CHUNK {
            let _ = self.producer.push(self.block);
            self.block.sequence = self.block.sequence.wrapping_add(1);
            self.used = 0;
        }
    }
}
impl Spectrum {
    pub fn set_active(&mut self, active: bool) {
        let state = self.state.load(Ordering::Relaxed);
        if (state & 1 != 0) == active {
            return;
        }
        self.state
            .store(state.wrapping_add(2) ^ 1, Ordering::Relaxed);
        self.cursor = 0;
        self.filled = 0;
        self.sequence = None;
        self.levels = [[FLOOR; BINS]; 2];
        // A callback already in progress may still publish one old block.
        // poll also checks its generation, so it cannot appear on reopening.
        for _ in 0..QUEUE {
            if self.consumer.pop().is_err() {
                break;
            }
        }
    }
    pub fn poll(&mut self) {
        let state = self.state.load(Ordering::Relaxed);
        if state & 1 == 0 {
            return;
        }
        let mut received = false;
        for _ in 0..QUEUE {
            let Ok(block) = self.consumer.pop() else {
                break;
            };
            if block.generation != state {
                continue;
            }
            if self
                .sequence
                .is_some_and(|s| s.wrapping_add(1) != block.sequence)
            {
                self.filled = 0;
                self.history = [[0.; FFT_SIZE]; 2];
                self.cursor = 0;
                self.levels = [[FLOOR; BINS]; 2];
            }
            self.sequence = Some(block.sequence);
            for frame in block.samples {
                for (ch, sample) in frame.into_iter().enumerate() {
                    self.history[ch][self.cursor] = if sample.is_finite() { sample } else { 0. };
                }
                self.cursor = (self.cursor + 1) % FFT_SIZE;
                self.filled = (self.filled + 1).min(FFT_SIZE);
            }
            received = true;
        }
        if received {
            self.last_data = Instant::now();
            if self.filled == FFT_SIZE {
                self.analyze();
            }
        } else if self.last_data.elapsed() > Duration::from_millis(250) {
            self.levels = [[FLOOR; BINS]; 2];
            self.filled = 0;
        }
    }
    pub fn levels(&self, ch: usize) -> Option<&[f32; BINS]> {
        (self.filled == FFT_SIZE).then_some(&self.levels[ch])
    }
    fn analyze(&mut self) {
        for ch in 0..2 {
            for i in 0..FFT_SIZE {
                self.real[i] = self.history[ch][(self.cursor + i) % FFT_SIZE] * self.window[i];
                self.imag[i] = 0.;
            }
            fft(&mut self.real, &mut self.imag);
            for i in 0..BINS {
                let edge = if i == 0 || i == FFT_SIZE / 2 { 0.5 } else { 1. };
                let magnitude = self.real[i].hypot(self.imag[i]) * self.scale * edge;
                let db = (20. * magnitude.max(1e-5).log10()).clamp(FLOOR, 12.);
                // Immediate attack keeps tone magnitudes calibrated; a gradual
                // release makes moving music readable without delaying audio.
                self.levels[ch][i] = db.max(self.levels[ch][i] - 6.);
            }
        }
    }
}
fn fft(real: &mut [f32; FFT_SIZE], imag: &mut [f32; FFT_SIZE]) {
    let mut j = 0;
    for i in 1..FFT_SIZE {
        let mut bit = FFT_SIZE >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= FFT_SIZE {
        let angle = -2. * PI / len as f32;
        let (wi, wr) = angle.sin_cos();
        for start in (0..FFT_SIZE).step_by(len) {
            let (mut ur, mut ui) = (1., 0.);
            for offset in 0..len / 2 {
                let a = start + offset;
                let b = a + len / 2;
                let vr = real[b] * ur - imag[b] * ui;
                let vi = real[b] * ui + imag[b] * ur;
                real[b] = real[a] - vr;
                imag[b] = imag[a] - vi;
                real[a] += vr;
                imag[a] += vi;
                (ur, ui) = (ur * wr - ui * wi, ur * wi + ui * wr);
            }
        }
        len *= 2;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suspended_capture_resumes_with_only_fresh_audio() {
        let (mut capture, mut spectrum) = channel();
        assert!(capture.begin_buffer());
        for _ in 0..CHUNK + 37 {
            capture.feed([1., 1.]);
        }
        spectrum.set_active(false);
        // Simulate the last callback racing with the visibility change.
        for _ in 0..CHUNK {
            capture.feed([1., 1.]);
        }
        assert!(!capture.begin_buffer());
        spectrum.poll();
        assert!(spectrum.levels(0).is_none());
        spectrum.set_active(true);
        assert!(capture.begin_buffer());
        for _ in 0..FFT_SIZE {
            capture.feed([0., 0.]);
        }
        spectrum.poll();
        assert!(spectrum.levels(0).unwrap().iter().all(|v| *v <= -99.));
    }
    #[test]
    fn separates_channels_and_calibrates_tone_levels() {
        let (mut capture, mut spectrum) = channel();
        for i in 0..FFT_SIZE {
            capture.feed([
                (2. * PI * 64. * i as f32 / FFT_SIZE as f32).sin() * 0.5,
                (2. * PI * 256. * i as f32 / FFT_SIZE as f32).sin() * 0.25,
            ]);
        }
        spectrum.poll();
        for (ch, bin, db) in [(0, 64, -6.0206), (1, 256, -12.0412)] {
            let levels = spectrum.levels(ch).unwrap();
            let peak = levels
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            assert_eq!(peak, bin);
            assert!((levels[bin] - db).abs() < 0.05);
        }
        assert!(spectrum.levels(0).unwrap()[256] < -80.);
    }
    #[test]
    fn queue_overflow_never_blocks_and_recovers_from_gaps() {
        let (mut capture, mut spectrum) = channel();
        for _ in 0..CHUNK * (QUEUE + 4) {
            capture.feed([0.1, -0.1]);
        }
        spectrum.poll();
        for _ in 0..CHUNK {
            capture.feed([0., 0.]);
        }
        spectrum.poll();
        assert!(spectrum.levels(0).is_none());
        for _ in 0..FFT_SIZE {
            capture.feed([0., 0.]);
        }
        spectrum.poll();
        assert!(spectrum.levels(0).unwrap().iter().all(|v| *v <= -99.));
    }
    #[test]
    fn startup_and_silence_are_finite_and_empty() {
        let (mut capture, mut spectrum) = channel();
        assert!(spectrum.levels(0).is_none());
        for _ in 0..FFT_SIZE {
            capture.feed([f32::NAN, 0.]);
        }
        spectrum.poll();
        assert!(spectrum
            .levels(0)
            .unwrap()
            .iter()
            .all(|v| v.is_finite() && *v <= -99.));
        spectrum.last_data = Instant::now() - Duration::from_secs(1);
        spectrum.poll();
        assert!(spectrum.levels(0).is_none());
    }
}
