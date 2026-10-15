//! Signal processing. Everything here is plain Rust with no dependencies,
//! so it is easy to unit test.

pub mod cracks;
pub mod fft;
pub mod formants;
pub mod notes;
pub mod pitch;
pub mod register;
pub mod undertone;

/// Root mean square level in dBFS, floored at -100.
pub fn rms_db(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return -100.0;
    }
    let s: f32 = buf.iter().map(|v| v * v).sum::<f32>() / buf.len() as f32;
    if s <= 0.0 {
        -100.0
    } else {
        (10.0 * s.log10()).max(-100.0)
    }
}

#[cfg(test)]
pub mod testsig {
    //! Synthetic voices for tests.
    use std::f64::consts::PI;

    pub fn sine(hz: f64, sr: f64, n: usize) -> Vec<f32> {
        (0..n).map(|i| (0.5 * (2.0 * PI * hz * i as f64 / sr).sin()) as f32).collect()
    }

    fn resonate(input: &[f64], fc: f64, bw: f64, sr: f64) -> Vec<f64> {
        let r = (-PI * bw / sr).exp();
        let c = 2.0 * r * (2.0 * PI * fc / sr).cos();
        let mut out = vec![0.0; input.len()];
        for i in 0..input.len() {
            let y1 = if i >= 1 { out[i - 1] } else { 0.0 };
            let y2 = if i >= 2 { out[i - 2] } else { 0.0 };
            out[i] = input[i] + c * y1 - r * r * y2;
        }
        out
    }

    /// Impulse train through three resonators: a crude but useful vowel.
    pub fn vowel(f0: f64, f1: f64, f2: f64, sr: f64, n: usize) -> Vec<f32> {
        let mut x = vec![0.0; n];
        let period = sr / f0;
        let mut p = 0.0;
        while (p as usize) < n {
            x[p as usize] = 1.0;
            p += period;
        }
        let y = resonate(&resonate(&resonate(&x, f1, 80.0, sr), f2, 100.0, sr), 2800.0, 150.0, sr);
        let max = y.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        y.iter().map(|v| (0.6 * v / max) as f32).collect()
    }

    /// Additive voice with controllable harmonic tilt.
    /// `h2_db` is the level of the 2nd harmonic relative to the 1st.
    pub fn harmonics(f0: f64, h2_db: f64, sr: f64, n: usize) -> Vec<f32> {
        let mut out = vec![0.0f64; n];
        for k in 1..=20 {
            let fk = f0 * k as f64;
            if fk > sr / 2.0 - 200.0 {
                break;
            }
            // 1st harmonic at 0 dB, 2nd at h2_db, then -6 dB/octave beyond
            let db = if k == 1 { 0.0 } else { h2_db - 6.0 * ((k as f64) / 2.0).log2() };
            let amp = 10f64.powf(db / 20.0);
            for (i, o) in out.iter_mut().enumerate() {
                *o += amp * (2.0 * PI * fk * i as f64 / sr).sin();
            }
        }
        let max = out.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        out.iter().map(|v| (0.5 * v / max) as f32).collect()
    }
}
