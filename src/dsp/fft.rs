//! Small in-place radix-2 FFT. Enough for one 4096-point frame per tick.

use std::f32::consts::PI;

/// In-place FFT of `re`/`im` (length must be a power of two).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    // bit reversal
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Hann-windowed magnitude spectrum (first half) in dB.
pub fn magnitude_db(buf: &[f32]) -> Vec<f32> {
    let n = buf.len().next_power_of_two();
    let mut re = vec![0f32; n];
    let mut im = vec![0f32; n];
    let m = buf.len();
    for i in 0..m {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (m - 1) as f32).cos();
        re[i] = buf[i] * w;
    }
    fft(&mut re, &mut im);
    (0..n / 2)
        .map(|k| 10.0 * (re[k] * re[k] + im[k] * im[k] + 1e-12).log10())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::testsig::sine;

    #[test]
    fn peak_at_right_bin() {
        let sr = 48000.0;
        let n = 4096;
        let hz = 1000.0;
        let spec = magnitude_db(&sine(hz, sr, n));
        let (bin, _) = spec
            .iter()
            .enumerate()
            .fold((0, f32::MIN), |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) });
        let got = bin as f64 * sr / n as f64;
        assert!((got - hz).abs() < sr / n as f64 * 1.5, "{got}");
    }
}
