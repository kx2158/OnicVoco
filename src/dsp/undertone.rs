//! Undertones: what is going on underneath the pitch.
//!
//! Three things voice trainers care about, all estimated from one frame:
//! - airy: breathiness, from how periodic the waveform is
//! - fry: creaky, rattly low voice (very low pitch, or a pulse train below the normal range)
//! - rough: sub-harmonics, extra energy at half the pitch (period doubling, strain, a voice on the edge of breaking)
//!
//! They are estimates. Microphones and rooms shift them, so watch the trend.

use super::fft::magnitude_db;
use super::pitch;

/// Points in the little undertone spectrum.
pub const SPEC_N: usize = 32;

/// Below this pitch, sub-harmonic analysis is skipped (mains hum lives at 50/60 Hz).
const MIN_F0_FOR_SUB: f32 = 130.0;

#[derive(Debug, Clone, Copy)]
pub struct Undertone {
    /// 0..1, breathiness.
    pub airy: f32,
    /// 0..1, creak / vocal fry.
    pub fry: f32,
    /// 0..1, sub-harmonics (roughness). `None` when the pitch is too low to measure reliably.
    pub rough: Option<f32>,
    /// Spectrum from 0.25x to 3.25x the pitch, 0..1 with the fundamental at 1.0.
    pub spectrum: [f32; SPEC_N],
    pub f0: f32,
}

pub fn airy_score(clarity: f32) -> f32 {
    ((0.95 - clarity) / 0.3).clamp(0.0, 1.0)
}

pub fn fry_score(f0: f32) -> f32 {
    ((82.0 - f0) / 22.0).clamp(0.0, 1.0)
}

pub fn rough_score(sub_db: f32) -> f32 {
    ((sub_db + 24.0) / 14.0).clamp(0.0, 1.0)
}

fn peak_in(spec: &[f32], bin_hz: f32, lo_hz: f32, hi_hz: f32) -> Option<f32> {
    let lo = (lo_hz / bin_hz).floor().max(0.0) as usize;
    let hi = (hi_hz / bin_hz).ceil() as usize;
    if hi >= spec.len() || lo > hi {
        return None;
    }
    spec[lo..=hi].iter().cloned().reduce(f32::max)
}

/// Level at half the pitch relative to the pitch itself, in dB.
fn sub_db(spec: &[f32], bin_hz: f32, f0: f32) -> Option<f32> {
    let h1 = peak_in(spec, bin_hz, f0 * 0.92, f0 * 1.08)?;
    let sub = peak_in(spec, bin_hz, f0 * 0.5 * 0.92, f0 * 0.5 * 1.08)?;
    Some(sub - h1)
}

/// Looks for a creaky pulse train below the normal pitch range.
pub fn creak_hz(buf: &[f32], sr: f32) -> Option<f32> {
    // A clearly periodic voice is also "periodic" at multiples of its period,
    // which would look like a very low pitch. Only call it creak if there is
    // no clean pitch in the normal range.
    if pitch::detect(buf, sr, 80.0, 1000.0).is_some_and(|p| p.clarity > 0.7) {
        return None;
    }
    pitch::detect(buf, sr, 35.0, 80.0).filter(|p| p.clarity > 0.5).map(|p| p.hz)
}

fn spectrum(spec: &[f32], bin_hz: f32, f0: f32) -> [f32; SPEC_N] {
    let mut out = [0f32; SPEC_N];
    let h1 = peak_in(spec, bin_hz, f0 * 0.92, f0 * 1.08).unwrap_or(-100.0);
    let (a, b) = (f0 * 0.25, f0 * 3.25);
    for (i, o) in out.iter_mut().enumerate() {
        let lo = a + (b - a) * i as f32 / SPEC_N as f32;
        let hi = a + (b - a) * (i + 1) as f32 / SPEC_N as f32;
        if let Some(v) = peak_in(spec, bin_hz, lo, hi) {
            *o = ((v - (h1 - 40.0)) / 40.0).clamp(0.0, 1.0);
        }
    }
    out
}

/// Analyse a frame with a known pitch and the pitch tracker's clarity.
pub fn analyze(buf: &[f32], sr: f32, f0: f32, clarity: f32) -> Undertone {
    let spec = magnitude_db(buf);
    let n = buf.len().next_power_of_two();
    let bin_hz = sr / n as f32;
    let rough = if f0 >= MIN_F0_FOR_SUB { sub_db(&spec, bin_hz, f0).map(rough_score) } else { None };
    Undertone { airy: airy_score(clarity), fry: fry_score(f0), rough, spectrum: spectrum(&spec, bin_hz, f0), f0 }
}

/// A frame where only creak was found (no normal pitch).
pub fn creaky(f0: f32) -> Undertone {
    Undertone { airy: 0.3, fry: 1.0, rough: None, spectrum: [0.0; SPEC_N], f0 }
}

pub fn describe(u: &Undertone) -> &'static str {
    let rough = u.rough.unwrap_or(0.0);
    if u.fry > 0.6 {
        "Creaky: that's vocal fry. Fine as a texture, but it's easy to slip into."
    } else if rough > 0.6 {
        "Rough: extra energy under the pitch. Often strain, or a voice about to flip."
    } else if u.airy > 0.6 {
        "Airy: lots of breath in the tone. Lovely for soft sounds, but easy to tire."
    } else if u.airy > 0.3 || rough > 0.3 {
        "Slightly textured"
    } else {
        "Clean and clear"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::testsig::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn run(sig: &[f32], f0: f32) -> Undertone {
        analyze(sig, SR as f32, f0, 0.95)
    }

    #[test]
    fn clean_voice_is_not_rough() {
        let u = run(&harmonics(200.0, -6.0, SR, 4096), 200.0);
        assert!(u.rough.unwrap() < 0.2, "{:?}", u.rough);
    }

    #[test]
    fn subharmonic_is_detected() {
        let base = harmonics(200.0, -6.0, SR, 4096);
        let sig: Vec<f32> = base
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let t = i as f64 / SR;
                x + 0.18 * (2.0 * PI * 100.0 * t).sin() as f32 + 0.1 * (2.0 * PI * 300.0 * t).sin() as f32
            })
            .collect();
        let u = run(&sig, 200.0);
        assert!(u.rough.unwrap() > 0.7, "{:?}", u.rough);
    }

    #[test]
    fn low_pitch_skips_sub_analysis() {
        let u = run(&harmonics(110.0, -6.0, SR, 4096), 110.0);
        assert!(u.rough.is_none());
    }

    #[test]
    fn airy_and_fry_scales() {
        assert!(airy_score(0.98) < 0.05);
        assert!(airy_score(0.65) > 0.95);
        assert!(airy_score(0.8) > airy_score(0.9));
        assert!(fry_score(120.0) == 0.0);
        assert!(fry_score(60.0) > 0.95);
    }

    #[test]
    fn creak_is_found_below_normal_range() {
        let sig = vowel(55.0, 500.0, 1500.0, SR, 4096);
        let hz = creak_hz(&sig, SR as f32).expect("creak");
        assert!((hz - 55.0).abs() < 3.0, "{hz}");
        // a normal voice is not creak
        let normal = vowel(200.0, 500.0, 1500.0, SR, 4096);
        assert!(creak_hz(&normal, SR as f32).is_none());
    }

    #[test]
    fn spectrum_peaks_at_the_fundamental() {
        let u = run(&harmonics(200.0, -6.0, SR, 4096), 200.0);
        // fundamental sits at (1 - 0.25) / 3.0 of the way across
        let idx = ((1.0 - 0.25) / 3.0 * SPEC_N as f32) as usize;
        let max = u.spectrum.iter().cloned().fold(0.0, f32::max);
        assert!(u.spectrum[idx] > 0.95 * max, "{:?}", u.spectrum);
    }
}
