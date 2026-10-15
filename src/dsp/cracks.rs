//! Voice crack detection.
//!
//! A crack is a sudden pitch jump of several semitones, for example flipping
//! from falsetto down into chest voice (or the other way). The voice sits on
//! one pitch level just before, and a clearly different level just after.
//!
//! Real cracks are messy: there is often a frame or two of in-between pitch,
//! or a tiny dropout, right at the break. So instead of comparing two
//! neighbouring frames, we compare the median of a few frames before the
//! jump with the median of a few frames after it, allowing up to two
//! transitional frames and very short gaps in between. Single-frame octave
//! errors from the pitch tracker are ignored because the new level has to
//! hold for at least two frames.

use super::notes::semitones;

#[derive(Debug, Clone, Copy)]
pub struct Crack {
    pub t: f32,
    pub from_hz: f32,
    pub to_hz: f32,
}

impl Crack {
    pub fn upward(&self) -> bool {
        self.to_hz > self.from_hz
    }
}

/// Smallest jump (semitones) that counts as a crack.
pub const MIN_JUMP_ST: f32 = 4.5;
/// Above this the jump is so large it is a crack even if the voice was a bit wobbly.
const HUGE_JUMP_ST: f32 = 8.0;
const STABLE_ST: f32 = 1.6;
const STABLE_HUGE_ST: f32 = 3.5;
/// Max silence inside a phrase. Longer gaps start a new phrase (not a crack).
const MAX_GAP_S: f32 = 0.16;
/// Max time between the last "before" frame and first "after" frame.
const MAX_BREAK_S: f32 = 0.18;
const DEBOUNCE_S: f32 = 0.5;
const BEFORE: usize = 3;
const AFTER: usize = 2;
const MAX_TRANSITION: usize = 2;

#[derive(Default)]
pub struct CrackDetector {
    hist: Vec<(f32, f32)>, // (t, raw hz) of recent voiced frames in this phrase
    last_voiced: f32,
    last_crack: f32,
}

fn median(v: &[f32]) -> f32 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[s.len() / 2]
}

fn spread_st(v: &[f32]) -> f32 {
    let lo = v.iter().cloned().fold(f32::MAX, f32::min);
    let hi = v.iter().cloned().fold(f32::MIN, f32::max);
    semitones(lo, hi)
}

impl CrackDetector {
    pub fn reset(&mut self) {
        self.hist.clear();
        self.last_voiced = f32::NEG_INFINITY;
        self.last_crack = f32::NEG_INFINITY;
    }

    /// Feed one analysis frame. `hz` is the raw (unsmoothed) pitch, None if unvoiced.
    pub fn push(&mut self, t: f32, hz: Option<f32>) -> Option<Crack> {
        let Some(hz) = hz else {
            // Brief dropouts at the break are kept; long silences end the phrase.
            if t - self.last_voiced > MAX_GAP_S {
                self.hist.clear();
            }
            return None;
        };
        if t - self.last_voiced > MAX_GAP_S {
            self.hist.clear();
        }
        self.last_voiced = t;
        self.hist.push((t, hz));
        let cap = BEFORE + MAX_TRANSITION + AFTER;
        if self.hist.len() > cap {
            self.hist.remove(0);
        }
        if t - self.last_crack < DEBOUNCE_S {
            return None;
        }

        let n = self.hist.len();
        let after: Vec<f32> = self.hist[n.saturating_sub(AFTER)..].iter().map(|f| f.1).collect();
        if after.len() < AFTER {
            return None;
        }
        for k in 0..=MAX_TRANSITION {
            if n < BEFORE + k + AFTER {
                break;
            }
            let b_end = n - AFTER - k; // exclusive
            let before_frames = &self.hist[b_end - BEFORE..b_end];
            let before: Vec<f32> = before_frames.iter().map(|f| f.1).collect();
            let (mb, ma) = (median(&before), median(&after));
            let jump = semitones(mb, ma);
            if jump.abs() < MIN_JUMP_ST {
                continue;
            }
            let stable = if jump.abs() >= HUGE_JUMP_ST { STABLE_HUGE_ST } else { STABLE_ST };
            let break_time = self.hist[n - AFTER].0 - before_frames[BEFORE - 1].0;
            if spread_st(&before) < stable && spread_st(&after) < stable && break_time <= MAX_BREAK_S {
                self.last_crack = t;
                let at = self.hist[n - AFTER].0;
                self.hist.drain(..n - AFTER); // the new level starts fresh
                return Some(Crack { t: at, from_hz: mb, to_hz: ma });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 0.033;

    fn run(seq: &[Option<f32>]) -> Vec<Crack> {
        let mut d = CrackDetector::default();
        d.reset();
        seq.iter().enumerate().filter_map(|(i, &h)| d.push(i as f32 * DT, h)).collect()
    }

    #[test]
    fn detects_register_flip_up() {
        let mut s: Vec<Option<f32>> = (0..10).map(|i| Some(250.0 + i as f32 * 3.0)).collect();
        s.extend((0..10).map(|i| Some(390.0 + i as f32 * 2.0)));
        let c = run(&s);
        assert_eq!(c.len(), 1);
        assert!(c[0].upward());
    }

    #[test]
    fn detects_falsetto_to_chest_drop_with_transition_and_wobble() {
        // Wobbly falsetto around 420 Hz, one in-between frame, a dropout, then chest at ~170 Hz.
        let mut s: Vec<Option<f32>> = vec![Some(410.0), Some(430.0), Some(415.0), Some(440.0), Some(420.0)];
        s.push(Some(290.0)); // transitional frame
        s.push(None); // momentary dropout at the break
        s.extend([Some(172.0), Some(168.0), Some(170.0), Some(171.0)]);
        let c = run(&s);
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(!c[0].upward());
        assert!(semitones(c[0].to_hz, c[0].from_hz) > 12.0);
    }

    #[test]
    fn short_falsetto_burst_then_chest() {
        // Like the screenshot: brief high segment, then a huge drop.
        let mut s: Vec<Option<f32>> = vec![Some(520.0), Some(505.0), Some(530.0)];
        s.extend([Some(150.0), Some(152.0), Some(149.0)]);
        assert_eq!(run(&s).len(), 1);
    }

    #[test]
    fn ignores_single_frame_octave_error() {
        let mut s: Vec<Option<f32>> = vec![Some(200.0); 6];
        s.push(Some(400.0));
        s.extend(vec![Some(200.0); 6]);
        assert!(run(&s).is_empty());
    }

    #[test]
    fn ignores_smooth_glide_and_new_phrases() {
        let glide: Vec<Option<f32>> = (0..60).map(|i| Some(150.0 * 2f32.powf(i as f32 / 40.0))).collect();
        assert!(run(&glide).is_empty());
        // A new phrase starting higher after a real pause is not a crack.
        let mut s = vec![Some(180.0); 6];
        s.extend(vec![None; 8]);
        s.extend(vec![Some(300.0); 6]);
        assert!(run(&s).is_empty());
    }

    #[test]
    fn natural_intonation_is_not_a_crack() {
        // Speech-like rises and falls of a few semitones.
        let s: Vec<Option<f32>> = (0..120).map(|i| Some(200.0 * 2f32.powf(0.25 * (i as f32 / 8.0).sin()))).collect();
        assert!(run(&s).is_empty());
    }
}
