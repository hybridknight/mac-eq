use crate::profile::{Band, Filter, Profile, MAX_BANDS};
use std::f64::consts::PI;
#[derive(Clone, Copy, Debug)]
pub struct Coeff(pub [f64; 5]);
impl Default for Coeff {
    fn default() -> Self {
        Self([1., 0., 0., 0., 0.])
    }
}
impl Coeff {
    pub fn design(b: &Band, rate: f64) -> Self {
        if !b.enabled {
            return Self::default();
        }
        let w = 2. * PI * b.hz.clamp(1., rate * 0.49) / rate;
        let (s, c) = w.sin_cos();
        let a = 10f64.powf(b.db / 40.);
        let alpha = s / (2. * b.q);
        let beta = 2. * a.sqrt() * alpha;
        let (b0, b1, b2, a0, a1, a2) = match b.kind {
            Filter::Peak => (
                1. + alpha * a,
                -2. * c,
                1. - alpha * a,
                1. + alpha / a,
                -2. * c,
                1. - alpha / a,
            ),
            Filter::LowPass => (
                (1. - c) / 2.,
                1. - c,
                (1. - c) / 2.,
                1. + alpha,
                -2. * c,
                1. - alpha,
            ),
            Filter::HighPass => (
                (1. + c) / 2.,
                -1. - c,
                (1. + c) / 2.,
                1. + alpha,
                -2. * c,
                1. - alpha,
            ),
            Filter::LowShelf => (
                a * ((a + 1.) - (a - 1.) * c + beta),
                2. * a * ((a - 1.) - (a + 1.) * c),
                a * ((a + 1.) - (a - 1.) * c - beta),
                (a + 1.) + (a - 1.) * c + beta,
                -2. * ((a - 1.) + (a + 1.) * c),
                (a + 1.) + (a - 1.) * c - beta,
            ),
            Filter::HighShelf => (
                a * ((a + 1.) + (a - 1.) * c + beta),
                -2. * a * ((a - 1.) + (a + 1.) * c),
                a * ((a + 1.) + (a - 1.) * c - beta),
                (a + 1.) - (a - 1.) * c + beta,
                2. * ((a - 1.) - (a + 1.) * c),
                (a + 1.) - (a - 1.) * c - beta,
            ),
        };
        Self([b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0])
    }
    pub fn magnitude(self, hz: f64, rate: f64) -> f64 {
        let w = 2. * PI * hz / rate;
        let [b0, b1, b2, a1, a2] = self.0;
        let re = b0 + b1 * w.cos() + b2 * (2. * w).cos();
        let im = -b1 * w.sin() - b2 * (2. * w).sin();
        let dr = 1. + a1 * w.cos() + a2 * (2. * w).cos();
        let di = -a1 * w.sin() - a2 * (2. * w).sin();
        ((re * re + im * im) / (dr * dr + di * di)).sqrt()
    }
}
#[derive(Clone, Copy)]
pub struct Params {
    coeff: [[Coeff; MAX_BANDS]; 2],
    gain: [f64; 2],
    wet: f64,
    limiter: bool,
}
impl Params {
    pub fn compile(profile: &Profile, rate: f64) -> Self {
        let mut p = Self {
            coeff: [[Coeff::default(); MAX_BANDS]; 2],
            gain: [1.; 2],
            wet: if profile.bypass { 0. } else { 1. },
            limiter: profile.limiter,
        };
        for ch in 0..2 {
            p.gain[ch] = 10f64.powf(profile.channels[ch].preamp / 20.);
            for (i, b) in profile.channels[ch]
                .bands
                .iter()
                .take(MAX_BANDS)
                .enumerate()
            {
                p.coeff[ch][i] = Coeff::design(b, rate);
            }
        }
        p
    }
}
struct Bank {
    params: Params,
    history: [[[f64; 2]; MAX_BANDS]; 2],
}
impl Bank {
    fn new(params: Params) -> Self {
        Self {
            params,
            history: [[[0.; 2]; MAX_BANDS]; 2],
        }
    }
    fn frame(&mut self, input: [f64; 2]) -> [f64; 2] {
        let mut out = input;
        for ch in 0..2 {
            let mut x = input[ch] * self.params.gain[ch];
            for i in 0..MAX_BANDS {
                let [b0, b1, b2, a1, a2] = self.params.coeff[ch][i].0;
                let h = &mut self.history[ch][i];
                let y = b0 * x + h[0];
                h[0] = b1 * x - a1 * y + h[1];
                h[1] = b2 * x - a2 * y;
                x = y;
            }
            if !x.is_finite() {
                self.history[ch] = [[0.; 2]; MAX_BANDS];
                x = 0.;
            }
            out[ch] = input[ch] + self.params.wet * (x - input[ch]);
        }
        out
    }
}
pub struct Processor {
    current: Bank,
    next: Option<Bank>,
    pending: Option<Params>,
    fade: usize,
    fade_len: usize,
    limiter_gain: f64,
    release: f64,
}
impl Processor {
    pub fn new(params: Params, rate: f64) -> Self {
        Self {
            current: Bank::new(params),
            next: None,
            pending: None,
            fade: 0,
            fade_len: (rate * 0.01).max(1.) as usize,
            limiter_gain: 1.,
            release: (-1. / (rate * 0.05)).exp(),
        }
    }
    pub fn update(&mut self, params: Params) {
        if self.next.is_some() {
            self.pending = Some(params);
        } else {
            self.next = Some(Bank::new(params));
            self.fade = 0;
        }
    }
    pub fn frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let input = input.map(|x| if x.is_finite() { x as f64 } else { 0. });
        let mut out = self.current.frame(input);
        if let Some(next) = &mut self.next {
            let other = next.frame(input);
            let t = (self.fade + 1) as f64 / self.fade_len as f64;
            for ch in 0..2 {
                out[ch] += t * (other[ch] - out[ch]);
            }
            self.fade += 1;
            if self.fade >= self.fade_len {
                self.current = self.next.take().unwrap();
                if let Some(p) = self.pending.take() {
                    self.update(p);
                }
            }
        }
        let peak = out[0].abs().max(out[1].abs());
        // Bypass preserves the dry signal. Limiting is active while wet processing contributes.
        let active = self.current.params.limiter
            && (self.current.params.wet > 0.
                || self.next.as_ref().is_some_and(|b| b.params.wet > 0.));
        if active {
            let desired = if peak > 0.98 { 0.98 / peak } else { 1. };
            self.limiter_gain = desired.min(1. - (1. - self.limiter_gain) * self.release);
            for v in &mut out {
                *v *= self.limiter_gain;
            }
        } else {
            self.limiter_gain = 1.;
        }
        out.map(|x| {
            let sample = x as f32;
            if sample.is_finite() {
                sample
            } else {
                0.
            }
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn flat() -> Profile {
        let mut p = Profile::default();
        for c in &mut p.channels {
            c.preamp = 0.;
        }
        p
    }
    #[test]
    fn flat_passthrough() {
        let mut d = Processor::new(Params::compile(&flat(), 48000.), 48000.);
        for _ in 0..1000 {
            let y = d.frame([0.25, -0.5]);
            assert!((y[0] - 0.25).abs() < 1e-6);
            assert!((y[1] + 0.5).abs() < 1e-6);
        }
    }
    #[test]
    fn peak_response() {
        let b = Band {
            db: 6.,
            ..Band::default()
        };
        assert!(
            (20. * Coeff::design(&b, 48000.).magnitude(1000., 48000.).log10() - 6.).abs() < 1e-8
        );
    }
    #[test]
    fn independent_channels() {
        let mut p = flat();
        p.linked = false;
        p.channels[0].bands[0].db = -12.;
        let mut d = Processor::new(Params::compile(&p, 48000.), 48000.);
        let mut power = [0.; 2];
        for i in 0..48000 {
            let x = (0.2 * (2. * PI * 1000. * i as f64 / 48000.).sin()) as f32;
            let y = d.frame([x, x]);
            if i > 1000 {
                for ch in 0..2 {
                    power[ch] += y[ch] as f64 * y[ch] as f64;
                }
            }
        }
        assert!((10. * (power[0] / power[1]).log10() + 12.).abs() < 0.01);
    }
    #[test]
    fn finite_and_limited() {
        let mut d = Processor::new(Params::compile(&flat(), 48000.), 48000.);
        for x in [f32::NAN, f32::INFINITY, 10.] {
            assert!(d
                .frame([x, x])
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 0.981));
        }
    }
    #[test]
    fn transitions_settle() {
        let p = flat();
        let mut d = Processor::new(Params::compile(&p, 48000.), 48000.);
        let mut q = p.clone();
        q.channels[0].preamp = -12.;
        d.update(Params::compile(&q, 48000.));
        for _ in 0..2000 {
            let y = d.frame([0.2, 0.2]);
            assert!(y[0].is_finite());
        }
        assert!((d.frame([0.2, 0.2])[0] - 0.2 * 10f32.powf(-12. / 20.)).abs() < 1e-5);
    }
    #[test]
    fn all_filters_stable_at_parameter_extremes() {
        for rate in [44100., 48000., 96000.] {
            for kind in Filter::ALL {
                for hz in [20., 1000., 20000.] {
                    for q in [0.1, 20.] {
                        for db in [-24., 24.] {
                            let mut p = flat();
                            for c in &mut p.channels {
                                c.bands[0] = Band {
                                    enabled: true,
                                    kind,
                                    hz,
                                    q,
                                    db,
                                };
                            }
                            let coeff = Coeff::design(&p.channels[0].bands[0], rate);
                            assert!(coeff.0.into_iter().all(f64::is_finite));
                            let [_, _, _, a1, a2] = coeff.0;
                            assert!(a2.abs() < 1. && 1. + a1 + a2 > 0. && 1. - a1 + a2 > 0.);
                            let mut processor = Processor::new(Params::compile(&p, rate), rate);
                            for i in 0..2000 {
                                let y = processor.frame(if i == 0 { [0.1, 0.1] } else { [0., 0.] });
                                assert!(y.into_iter().all(|v| v.is_finite() && v.abs() <= 0.981));
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn bypass_is_dry_after_transition() {
        let mut p = flat();
        p.channels[0].preamp = -20.;
        p.bypass = true;
        let mut d = Processor::new(Params::compile(&p, 48000.), 48000.);
        assert_eq!(d.frame([0.7, -0.4]), [0.7, -0.4]);
    }
    #[test]
    fn shelf_and_pass_endpoints() {
        let b = Band {
            kind: Filter::LowShelf,
            db: 6.,
            ..Band::default()
        };
        let c = Coeff::design(&b, 48000.);
        assert!((20. * c.magnitude(1., 48000.).log10() - 6.).abs() < 0.01);
        let hp = Coeff::design(
            &Band {
                kind: Filter::HighPass,
                ..b
            },
            48000.,
        );
        assert!(hp.magnitude(10., 48000.) < 0.001);
    }
    #[test]
    fn unprotected_extreme_input_never_emits_infinity() {
        let mut p = flat();
        p.limiter = false;
        for c in &mut p.channels {
            c.preamp = 12.;
        }
        let mut d = Processor::new(Params::compile(&p, 48000.), 48000.);
        assert!(d
            .frame([f32::MAX, f32::MAX])
            .into_iter()
            .all(f32::is_finite));
    }
}
