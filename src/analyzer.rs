//! Turns the latest microphone samples into one analysis frame.

use crate::dsp::cracks::{Crack, CrackDetector};
use crate::dsp::formants::{self, Formants};
use crate::dsp::register::{self, Register, RegisterTracker};
use crate::dsp::{pitch, rms_db};

pub const FRAME: usize = 4096;
const PITCH_FRAME: usize = 2048;

#[derive(Debug, Clone, Copy, Default)]
pub struct LiveFrame {
    pub t: f32,
    pub db: f32,
    /// Median-smoothed pitch.
    pub hz: Option<f32>,
    pub res: Option<f32>,
    pub formants: Option<Formants>,
    pub h1h2: Option<f32>,
    pub register: Option<Register>,
    pub crack: Option<Crack>,
}

pub struct Analyzer {
    pub gate_db: f32,
    recent: Vec<f32>,
    res_ema: Option<f32>,
    reg: RegisterTracker,
    cracks: CrackDetector,
}

impl Analyzer {
    pub fn new(gate_db: f32) -> Self {
        let mut a = Analyzer {
            gate_db,
            recent: Vec::new(),
            res_ema: None,
            reg: RegisterTracker::default(),
            cracks: CrackDetector::default(),
        };
        a.reset();
        a
    }

    pub fn reset(&mut self) {
        self.recent.clear();
        self.res_ema = None;
        self.reg.reset();
        self.cracks.reset();
    }

    /// `buf` must hold the newest FRAME samples.
    pub fn analyze(&mut self, buf: &[f32], sr: f32, t: f32) -> LiveFrame {
        let mut f = LiveFrame { t, db: rms_db(buf), ..Default::default() };
        let mut raw = None;
        if f.db > self.gate_db {
            let p = pitch::detect(&buf[buf.len() - PITCH_FRAME..], sr, 60.0, 900.0);
            if let Some(p) = p.filter(|p| p.clarity > 0.8) {
                raw = Some(p.hz);
                f.hz = Some(self.stabilise(p.hz));
                if let Some(fm) = formants::estimate(buf, sr) {
                    let s = formants::resonance_score(fm);
                    let e = self.res_ema.map_or(s, |prev| prev * 0.85 + s * 0.15);
                    self.res_ema = Some(e);
                    f.res = Some(e);
                    f.formants = Some(fm);
                }
                if let Some(h) = register::h1_h2(buf, sr, p.hz) {
                    f.register = Some(self.reg.push(h));
                    f.h1h2 = self.reg.smoothed();
                }
            }
        }
        if raw.is_none() {
            self.recent.clear();
        }
        f.crack = self.cracks.push(t, raw);
        f
    }

    // Median of the last few readings removes single-frame octave jumps.
    fn stabilise(&mut self, hz: f32) -> f32 {
        self.recent.push(hz);
        if self.recent.len() > 5 {
            self.recent.remove(0);
        }
        let mut s = self.recent.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[s.len() / 2]
    }
}
