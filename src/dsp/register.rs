//! Vocal register estimate (chest / mix / head-falsetto) from H1-H2:
//! the level difference between the first and second harmonics.
//! Chest voice has strong upper harmonics (H1-H2 low or negative);
//! falsetto and light head voice are dominated by the fundamental (H1-H2 high).
//! This is a well-known acoustic correlate, but microphones and vowels shift
//! it, so the app treats it as an estimate and smooths it.

use super::fft::magnitude_db;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    Chest,
    Mix,
    Head,
}

impl Register {
    pub fn label(self) -> &'static str {
        match self {
            Register::Chest => "Chest",
            Register::Mix => "Mix / passaggio",
            Register::Head => "Head / falsetto",
        }
    }
}

pub const CHEST_MAX_DB: f32 = 4.0;
pub const HEAD_MIN_DB: f32 = 10.0;

fn peak_near(spec: &[f32], bin_hz: f32, target: f32, spread: f32) -> Option<f32> {
    let lo = ((target * (1.0 - spread)) / bin_hz).floor() as usize;
    let hi = ((target * (1.0 + spread)) / bin_hz).ceil() as usize;
    if hi >= spec.len() || lo >= hi {
        return None;
    }
    spec[lo..=hi].iter().cloned().reduce(f32::max)
}

/// H1-H2 in dB for a frame with known fundamental `f0`.
pub fn h1_h2(buf: &[f32], sr: f32, f0: f32) -> Option<f32> {
    let spec = magnitude_db(buf);
    let n = buf.len().next_power_of_two();
    let bin_hz = sr / n as f32;
    let h1 = peak_near(&spec, bin_hz, f0, 0.12)?;
    let h2 = peak_near(&spec, bin_hz, 2.0 * f0, 0.08)?;
    Some(h1 - h2)
}

pub fn classify(h1h2: f32) -> Register {
    if h1h2 < CHEST_MAX_DB {
        Register::Chest
    } else if h1h2 > HEAD_MIN_DB {
        Register::Head
    } else {
        Register::Mix
    }
}

/// Smooths H1-H2 over time and adds hysteresis so the label does not flicker.
#[derive(Default)]
pub struct RegisterTracker {
    ema: Option<f32>,
    current: Option<Register>,
}

impl RegisterTracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn smoothed(&self) -> Option<f32> {
        self.ema
    }

    pub fn push(&mut self, h1h2: f32) -> Register {
        let e = match self.ema {
            Some(prev) => prev * 0.75 + h1h2 * 0.25,
            None => h1h2,
        };
        self.ema = Some(e);
        let hyst = 1.5;
        let next = match self.current {
            Some(Register::Chest) if e < CHEST_MAX_DB + hyst => Register::Chest,
            Some(Register::Head) if e > HEAD_MIN_DB - hyst => Register::Head,
            _ => classify(e),
        };
        self.current = Some(next);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::testsig::harmonics;

    #[test]
    fn measures_h1_h2() {
        for (f0, h2) in [(150.0, 3.0), (200.0, -6.0), (330.0, -15.0)] {
            let buf = harmonics(f0, h2, 48000.0, 4096);
            let got = h1_h2(&buf, 48000.0, f0 as f32).unwrap();
            assert!((got - (-h2 as f32)).abs() < 2.0, "f0 {f0} expected {} got {got}", -h2);
        }
    }

    #[test]
    fn classifies_registers() {
        let chest = harmonics(140.0, 2.0, 48000.0, 4096);
        let head = harmonics(400.0, -16.0, 48000.0, 4096);
        assert_eq!(classify(h1_h2(&chest, 48000.0, 140.0).unwrap()), Register::Chest);
        assert_eq!(classify(h1_h2(&head, 48000.0, 400.0).unwrap()), Register::Head);
    }
}
