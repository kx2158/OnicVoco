//! Voice crack detection.
//! A crack is a sudden pitch jump of several semitones between two
//! consecutive voiced frames, where the voice is stable just before and just
//! after the jump. Requiring stability on both sides filters out single-frame
//! octave errors from the pitch tracker and normal expressive glides.

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
    pub fn size_semitones(&self) -> f32 {
        semitones(self.from_hz, self.to_hz).abs()
    }
}

pub const MIN_JUMP_ST: f32 = 4.0;
const STABLE_ST: f32 = 1.2;
const MAX_GAP_S: f32 = 0.09;
const DEBOUNCE_S: f32 = 0.6;

#[derive(Default)]
pub struct CrackDetector {
    hist: Vec<(f32, f32)>, // (t, raw hz) of recent contiguous voiced frames
    last_crack: f32,
}

impl CrackDetector {
    pub fn reset(&mut self) {
        self.hist.clear();
        self.last_crack = f32::NEG_INFINITY;
    }

    /// Feed one analysis frame. `hz` is the raw (unsmoothed) pitch, None if unvoiced.
    pub fn push(&mut self, t: f32, hz: Option<f32>) -> Option<Crack> {
        let Some(hz) = hz else {
            self.hist.clear();
            return None;
        };
        if let Some(&(pt, _)) = self.hist.last() {
            if t - pt > MAX_GAP_S {
                self.hist.clear();
            }
        }
        self.hist.push((t, hz));
        if self.hist.len() > 4 {
            self.hist.remove(0);
        }
        if self.hist.len() < 4 {
            return None;
        }
        let [(_, a), (_, b), (_, c), (_, d)] = [self.hist[0], self.hist[1], self.hist[2], self.hist[3]];
        let jump = semitones(b, c);
        let stable_before = semitones(a, b).abs() < STABLE_ST;
        let stable_after = semitones(c, d).abs() < STABLE_ST;
        if jump.abs() >= MIN_JUMP_ST && stable_before && stable_after && t - self.last_crack > DEBOUNCE_S {
            self.last_crack = t;
            return Some(Crack { t: self.hist[2].0, from_hz: b, to_hz: c });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seq: &[Option<f32>]) -> Vec<Crack> {
        let mut d = CrackDetector::default();
        d.reset();
        seq.iter().enumerate().filter_map(|(i, &h)| d.push(i as f32 * 0.033, h)).collect()
    }

    #[test]
    fn detects_register_flip() {
        let mut s: Vec<Option<f32>> = (0..10).map(|i| Some(250.0 + i as f32 * 3.0)).collect();
        s.extend((0..10).map(|i| Some(390.0 + i as f32 * 2.0))); // ~7 semitone flip
        let c = run(&s);
        assert_eq!(c.len(), 1);
        assert!(c[0].upward());
    }

    #[test]
    fn ignores_single_frame_octave_error() {
        let mut s: Vec<Option<f32>> = vec![Some(200.0); 6];
        s.push(Some(400.0));
        s.extend(vec![Some(200.0); 6]);
        assert!(run(&s).is_empty());
    }

    #[test]
    fn ignores_smooth_glide_and_gaps() {
        let glide: Vec<Option<f32>> = (0..60).map(|i| Some(150.0 * 2f32.powf(i as f32 / 40.0))).collect();
        assert!(run(&glide).is_empty());
        let mut s = vec![Some(180.0); 5];
        s.push(None);
        s.extend(vec![Some(300.0); 5]); // new phrase starting higher is not a crack
        assert!(run(&s).is_empty());
    }
}
