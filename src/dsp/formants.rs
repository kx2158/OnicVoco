//! Formant estimation via LPC. Fits an all-pole model to a voiced frame,
//! evaluates its envelope and picks the first two prominent peaks.
//! Formants depend on the vowel, so callers should smooth over time.

const TARGET_RATE: f32 = 11025.0;
const MIN_PROMINENCE_DB: f64 = 2.0;

fn downsample(buf: &[f32], sr: f32) -> (Vec<f64>, f64) {
    let factor = ((sr / TARGET_RATE).round() as usize).max(1);
    let n = buf.len() / factor;
    let out = (0..n)
        .map(|i| buf[i * factor..(i + 1) * factor].iter().map(|&v| v as f64).sum::<f64>() / factor as f64)
        .collect();
    (out, sr as f64 / factor as f64)
}

/// Levinson-Durbin recursion. Returns a[0..=order] with a[0] = 1.
pub fn levinson(r: &[f64], order: usize) -> Option<Vec<f64>> {
    let mut a = vec![0f64; order + 1];
    a[0] = 1.0;
    let mut err = r[0];
    if err <= 0.0 {
        return None;
    }
    for i in 1..=order {
        let mut acc = r[i];
        for j in 1..i {
            acc += a[j] * r[i - j];
        }
        let k = -acc / err;
        let prev = a.clone();
        for j in 1..i {
            a[j] = prev[j] + k * prev[i - j];
        }
        a[i] = k;
        err *= 1.0 - k * k;
        if err <= 0.0 {
            return None;
        }
    }
    Some(a)
}

fn lpc(x: &[f64], order: usize) -> Option<Vec<f64>> {
    let n = x.len();
    let w: Vec<f64> = (0..n)
        .map(|i| {
            let pre = if i == 0 { x[0] } else { x[i] - 0.97 * x[i - 1] };
            pre * (0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos())
        })
        .collect();
    let mut r = vec![0f64; order + 1];
    for lag in 0..=order {
        r[lag] = (lag..n).map(|i| w[i] * w[i - lag]).sum();
    }
    r[0] *= 1.0 + 1e-9;
    levinson(&r, order)
}

#[derive(Debug, Clone, Copy)]
pub struct Formants {
    pub f1: f32,
    pub f2: f32,
}

pub fn estimate(buf: &[f32], sr: f32) -> Option<Formants> {
    let (data, rate) = downsample(buf, sr);
    if data.len() < 128 {
        return None;
    }
    let order = (2.0 + rate / 1000.0).round() as usize;
    let a = lpc(&data, order)?;

    let freqs: Vec<f64> = (0..=385).map(|i| 150.0 + 10.0 * i as f64).collect(); // 150..4000 Hz
    let env: Vec<f64> = freqs
        .iter()
        .map(|&f| {
            let w = 2.0 * std::f64::consts::PI * f / rate;
            let (mut re, mut im) = (0.0, 0.0);
            for (k, &ak) in a.iter().enumerate() {
                re += ak * (w * k as f64).cos();
                im -= ak * (w * k as f64).sin();
            }
            -10.0 * (re * re + im * im + 1e-12).log10()
        })
        .collect();

    let mut peaks = Vec::new();
    for i in 1..env.len() - 1 {
        if !(env[i] > env[i - 1] && env[i] >= env[i + 1]) {
            continue;
        }
        let mut l = i;
        while l > 0 && env[l - 1] <= env[l] {
            l -= 1;
        }
        let mut r = i;
        while r < env.len() - 1 && env[r + 1] <= env[r] {
            r += 1;
        }
        if env[i] - env[l].max(env[r]) >= MIN_PROMINENCE_DB {
            peaks.push(freqs[i]);
        }
    }
    let f1 = *peaks.iter().find(|&&f| (200.0..=1100.0).contains(&f))?;
    let f2 = *peaks.iter().find(|&&f| f > f1 + 250.0 && (700.0..=3200.0).contains(&f))?;
    Some(Formants { f1: f1 as f32, f2: f2 as f32 })
}

/// 0 = darker, 1 = brighter. A relative trend, not a verdict.
pub fn resonance_score(f: Formants) -> f32 {
    let s2 = ((f.f2 - 1150.0) / (1950.0 - 1150.0)).clamp(0.0, 1.0);
    let s1 = ((f.f1 - 380.0) / (720.0 - 380.0)).clamp(0.0, 1.0);
    (0.75 * s2 + 0.25 * s1).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::testsig::vowel;

    #[test]
    fn recovers_f1_f2() {
        for (f0, f1, f2) in [(120.0, 700.0, 1220.0), (200.0, 850.0, 2000.0), (180.0, 400.0, 2100.0), (240.0, 900.0, 2200.0)] {
            let est = estimate(&vowel(f0, f1, f2, 48000.0, 4096), 48000.0).expect("formants");
            assert!((est.f1 as f64 - f1).abs() < 120.0, "F1 {f1} -> {}", est.f1);
            assert!((est.f2 as f64 - f2).abs() < 200.0, "F2 {f2} -> {}", est.f2);
        }
    }

    #[test]
    fn brighter_scores_higher() {
        let dark = resonance_score(Formants { f1: 450.0, f2: 1200.0 });
        let bright = resonance_score(Formants { f1: 680.0, f2: 1900.0 });
        assert!(dark < 0.2 && bright > 0.8);
    }
}
