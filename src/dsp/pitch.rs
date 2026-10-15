//! YIN pitch detection (de Cheveigné & Kawahara, 2002).

#[derive(Debug, Clone, Copy)]
pub struct Pitch {
    pub hz: f32,
    /// 0..1, how periodic the frame is.
    pub clarity: f32,
}

pub fn detect(buf: &[f32], sample_rate: f32, min_hz: f32, max_hz: f32) -> Option<Pitch> {
    // Clean voices dip well below this; breathy falsetto often doesn't.
    let threshold = 0.2f32;
    let breathy_limit = 0.4f32;
    let max_tau = ((sample_rate / min_hz) as usize).min(buf.len() / 2);
    let min_tau = ((sample_rate / max_hz) as usize).max(2);
    if max_tau <= min_tau || buf.len() <= max_tau {
        return None;
    }
    let w = buf.len() - max_tau;

    // difference function
    let mut d = vec![0f32; max_tau + 1];
    for tau in 1..=max_tau {
        let mut sum = 0f32;
        let a = &buf[..w];
        let b = &buf[tau..tau + w];
        for i in 0..w {
            let delta = a[i] - b[i];
            sum += delta * delta;
        }
        d[tau] = sum;
    }

    // cumulative mean normalised difference
    let mut cmnd = vec![1f32; max_tau + 1];
    let mut running = 0f32;
    for tau in 1..=max_tau {
        running += d[tau];
        cmnd[tau] = if running > 0.0 { d[tau] * tau as f32 / running } else { 1.0 };
    }

    // absolute threshold, then walk down to the local minimum
    let mut tau = None;
    let mut t = min_tau;
    while t <= max_tau {
        if cmnd[t] < threshold {
            while t + 1 <= max_tau && cmnd[t + 1] < cmnd[t] {
                t += 1;
            }
            tau = Some(t);
            break;
        }
        t += 1;
    }
    // Breathy voices (falsetto!) may never cross the threshold. Fall back to
    // the deepest dip if it is still clearly periodic.
    let tau = match tau {
        Some(t) => t,
        None => {
            let (mut best, mut best_v) = (0usize, f32::MAX);
            for t in min_tau..=max_tau {
                if cmnd[t] < best_v {
                    best_v = cmnd[t];
                    best = t;
                }
            }
            if best_v >= breathy_limit {
                return None;
            }
            best
        }
    };

    // parabolic interpolation
    let mut better = tau as f32;
    if tau > 1 && tau < max_tau {
        let (s0, s1, s2) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
        let denom = 2.0 * (2.0 * s1 - s2 - s0);
        if denom != 0.0 {
            better = tau as f32 + (s2 - s0) / denom;
        }
    }
    let hz = sample_rate / better;
    if hz < min_hz || hz > max_hz {
        return None;
    }
    Some(Pitch { hz, clarity: (1.0 - cmnd[tau]).clamp(0.0, 1.0) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::testsig::*;

    const SR: f64 = 48000.0;

    #[test]
    fn sines() {
        for hz in [70.0, 98.0, 130.8, 196.0, 220.0, 261.6, 440.0] {
            let p = detect(&sine(hz, SR, 2048), SR as f32, 60.0, 1000.0).expect("pitch");
            assert!(((p.hz as f64 - hz) / hz).abs() < 0.01, "{hz} -> {}", p.hz);
        }
    }

    #[test]
    fn vowels() {
        for f0 in [110.0, 150.0, 200.0, 240.0] {
            let p = detect(&vowel(f0, 600.0, 1700.0, SR, 2048), SR as f32, 60.0, 1000.0).expect("pitch");
            assert!(((p.hz as f64 - f0) / f0).abs() < 0.02, "{f0} -> {}", p.hz);
        }
    }

    #[test]
    fn breathy_voice_still_tracked() {
        // A vowel with a lot of noise mixed in, like airy falsetto.
        let mut seed = 99u32;
        let v = vowel(330.0, 500.0, 1900.0, SR, 2048);
        let mixed: Vec<f32> = v
            .iter()
            .map(|x| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                x + 0.12 * ((seed as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect();
        let p = detect(&mixed, SR as f32, 60.0, 1000.0).expect("pitch");
        assert!((p.hz - 330.0).abs() / 330.0 < 0.03, "{}", p.hz);
    }

    #[test]
    fn silence_and_noise() {
        assert!(detect(&vec![0.0; 2048], SR as f32, 60.0, 1000.0).is_none());
        let mut seed = 12345u32;
        let noise: Vec<f32> = (0..2048)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect();
        assert!(detect(&noise, SR as f32, 60.0, 1000.0).is_none());
    }
}
