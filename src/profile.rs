//! Your voice profile: a vocal range map and a skill web that fill in slowly
//! while you use the app. Everything is stored on this computer only.
//!
//! Nothing here is a score to chase. Each axis is just a running average of
//! something the app already measures, so the shape of the web shows where your
//! voice is strong and where it has room to grow.

use crate::analyzer::LiveFrame;
use crate::dsp::notes::{hz_to_midi, semitones};
use crate::dsp::register::Register;
use crate::session::Target;
#[cfg(not(nodeps))]
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Range map covers C2 to C6.
pub const NOTE_MIN: i32 = 36;
pub const NOTE_MAX: i32 = 84;
/// A note counts as part of your range once you've sounded it this long.
pub const MAPPED_SECS: f32 = 0.4;

const PITCH: usize = 0;
const STEADY: usize = 1;
const RES: usize = 2;
const SMOOTH: usize = 3;
const CLEAN: usize = 4;
/// Axes that are measured as you go. Range and habit are derived.
const ACC_N: usize = 5;
pub const AXIS_COUNT: usize = 7;

/// A running average with a memory of how much evidence is behind it.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(not(nodeps), derive(Serialize, Deserialize))]
pub struct Acc {
    pub value: f32,
    /// Seconds of evidence (with gentle forgetting).
    pub weight: f32,
}

impl Acc {
    fn add(&mut self, obs: f32, w: f32) {
        if w <= 0.0 {
            return;
        }
        // Recent practice counts a bit more than old practice.
        let keep = 0.9994f32.powf(w);
        let old = self.weight * keep;
        let total = old + w;
        self.value = (self.value * old + obs.clamp(0.0, 1.0) * w) / total;
        self.weight = total;
    }

    pub fn confidence(&self) -> f32 {
        1.0 - (-self.weight / 90.0).exp()
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(not(nodeps), derive(Serialize, Deserialize))]
pub struct NoteStat {
    pub secs: f32,
    pub chest: f32,
    pub mix: f32,
    pub head: f32,
    pub cracks: f32,
}

impl NoteStat {
    pub fn mapped(&self) -> bool {
        self.secs >= MAPPED_SECS
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(nodeps), derive(Serialize, Deserialize))]
#[cfg_attr(not(nodeps), serde(default))]
pub struct Profile {
    pub acc: [Acc; ACC_N],
    /// One entry per semitone from NOTE_MIN.
    pub notes: Vec<NoteStat>,
    /// (day number, voiced seconds), most recent last.
    pub days: Vec<(i64, f32)>,
    #[cfg_attr(not(nodeps), serde(skip))]
    recent: VecDeque<f32>,
    #[cfg_attr(not(nodeps), serde(skip))]
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct AxisView {
    pub name: &'static str,
    pub hint: &'static str,
    pub value: f32,
    /// 0..1, how much evidence there is. Low means "needs more practice to say".
    pub confidence: f32,
}

const AXES: [(&str, &str); AXIS_COUNT] = [
    ("Pitch", "Time spent inside your goal pitch range."),
    ("Steadiness", "How little your pitch wanders. Speech moves naturally, so watch the trend, not the number."),
    ("Resonance", "Time with your resonance near your goal's zone (needs a goal with a resonance zone)."),
    ("Smooth breaks", "Fewer cracks and sudden register flips."),
    ("Clean tone", "Little fry, roughness or excess air."),
    ("Range", "Width of the pitch range you've mapped. Two octaves fills this axis."),
    ("Habit", "Days you've practised in the last two weeks."),
];

/// What to try for each axis when it's the one with the most room to grow.
const FOCUS_TIPS: [&str; AXIS_COUNT] = [
    "Try reading a passage on the Read tab with the goal range shaded, and anchor the ends of your sentences inside it.",
    "Hold a gentle \"oo\" on one note for ten seconds and watch the line stay flat. Short, easy sirens help too.",
    "Smile slightly and lift the back of your tongue toward \"ee\" shapes. Watch the resonance meter drift brighter.",
    "Practise sirens across your passaggio very quietly, and lighten up as you approach the note where you crack.",
    "Aim for gentle, supported air. If you hear rattle or strain, drop the volume and ease off.",
    "Do slow glides from your lowest comfortable note to your highest and let the range map fill in.",
    "Even five minutes a day beats one long session. Leave the mic live while you practise.",
];

pub fn focus_tip(axis: usize) -> &'static str {
    FOCUS_TIPS.get(axis).copied().unwrap_or("")
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

impl Profile {
    pub fn new() -> Self {
        Profile { notes: vec![NoteStat::default(); (NOTE_MAX - NOTE_MIN + 1) as usize], ..Default::default() }
    }

    fn fix_len(&mut self) {
        let n = (NOTE_MAX - NOTE_MIN + 1) as usize;
        if self.notes.len() != n {
            self.notes.resize(n, NoteStat::default());
        }
    }

    /// Call after loading from disk.
    pub fn prepare(&mut self) {
        self.fix_len();
    }

    fn note_index(hz: f32) -> Option<usize> {
        let m = hz_to_midi(hz).round() as i32;
        (NOTE_MIN..=NOTE_MAX).contains(&m).then(|| (m - NOTE_MIN) as usize)
    }

    /// Feeds one analysis frame. `dt` is the time it represents, `day` today's day number.
    pub fn observe(&mut self, f: &LiveFrame, dt: f32, target: &Target, day: i64) {
        self.observe_with(f, dt, target, day, true);
    }

    /// Like `observe`, but with `web: false` only the range map and practice
    /// days are updated. Used for glides, which would skew the skill web.
    pub fn observe_with(&mut self, f: &LiveFrame, dt: f32, target: &Target, day: i64, web: bool) {
        let dt = dt.clamp(0.0, 0.1);
        self.fix_len();

        if let Some(c) = f.crack {
            if web {
                // A crack counts as several seconds of rough going.
                self.acc[SMOOTH].add(0.0, 10.0);
            }
            if let Some(i) = Self::note_index(c.from_hz.min(c.to_hz)) {
                self.notes[i].cracks += 1.0;
            }
            self.dirty = true;
        }

        if let (true, Some(u)) = (web, f.under) {
            let bad = u.fry.max(u.rough.unwrap_or(0.0)).max(u.airy * 0.5);
            self.acc[CLEAN].add(1.0 - bad, dt);
            self.dirty = true;
        }

        let Some(hz) = f.hz else {
            self.recent.clear();
            return;
        };

        // practice time per day
        match self.days.last_mut() {
            Some(d) if d.0 == day => d.1 += dt,
            _ => {
                self.days.push((day, dt));
                let keep_from = day - 60;
                self.days.retain(|d| d.0 >= keep_from);
            }
        }

        if web {
            self.acc[PITCH].add(if target.contains(hz) { 1.0 } else { 0.0 }, dt);
            self.acc[SMOOTH].add(1.0, dt);

            self.recent.push_back(hz);
            if self.recent.len() > 45 {
                self.recent.pop_front();
            }
            if self.recent.len() >= 10 {
                let mut s: Vec<f32> = self.recent.iter().cloned().collect();
                s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let med = s[s.len() / 2];
                let dev = semitones(med, hz).abs();
                self.acc[STEADY].add(clamp01(1.0 - dev / 3.0), dt);
            }

            if let (Some(r), Some((lo, hi, _))) = (f.res, target.res) {
                let dist = if r < lo { lo - r } else if r > hi { r - hi } else { 0.0 };
                self.acc[RES].add(clamp01(1.0 - dist / 0.3), dt);
            }
        }

        if let Some(i) = Self::note_index(hz) {
            let n = &mut self.notes[i];
            n.secs += dt;
            match f.register {
                Some(Register::Chest) => n.chest += dt,
                Some(Register::Mix) => n.mix += dt,
                Some(Register::Head) => n.head += dt,
                None => {}
            }
        }
        self.dirty = true;
    }

    // ----- range -----

    /// Lowest and highest mapped notes (MIDI numbers).
    pub fn range(&self) -> Option<(i32, i32)> {
        let mut it = self.notes.iter().enumerate().filter(|(_, n)| n.mapped()).map(|(i, _)| i as i32 + NOTE_MIN);
        let first = it.next()?;
        let last = it.last()?;
        Some((first, last))
    }

    pub fn mapped_count(&self) -> usize {
        self.notes.iter().filter(|n| n.mapped()).count()
    }

    /// The notes where most of the time is spent (smallest span covering about 60% of it).
    pub fn comfort_zone(&self) -> Option<(i32, i32)> {
        let total: f32 = self.notes.iter().map(|n| n.secs).sum();
        if total < 5.0 {
            return None;
        }
        let want = total * 0.6;
        let n = self.notes.len();
        let mut best: Option<(usize, usize)> = None;
        for a in 0..n {
            let mut acc = 0.0;
            for b in a..n {
                acc += self.notes[b].secs;
                if acc >= want {
                    if best.map_or(true, |(x, y)| b - a < y - x) {
                        best = Some((a, b));
                    }
                    break;
                }
            }
        }
        best.map(|(a, b)| (a as i32 + NOTE_MIN, b as i32 + NOTE_MIN))
    }

    /// A note passed through cleanly in a guided glide: it counts as mapped
    /// even though a glide doesn't dwell on any single note.
    pub fn touch(&mut self, midi: i32) {
        self.fix_len();
        if (NOTE_MIN..=NOTE_MAX).contains(&midi) {
            let n = &mut self.notes[(midi - NOTE_MIN) as usize];
            if n.secs < MAPPED_SECS {
                n.secs = MAPPED_SECS;
                self.dirty = true;
            }
        }
    }

    /// Clears the skill web and practice days (the range map is kept).
    pub fn reset_web(&mut self) {
        self.acc = Default::default();
        self.days.clear();
        self.recent.clear();
        self.dirty = true;
    }

    pub fn reset_range(&mut self) {
        for n in self.notes.iter_mut() {
            *n = NoteStat::default();
        }
        self.dirty = true;
    }

    // ----- skill web -----

    pub fn habit_days(&self, today: i64) -> usize {
        self.days.iter().filter(|d| d.0 > today - 14 && d.0 <= today && d.1 >= 120.0).count()
    }

    pub fn axes(&self, today: i64) -> Vec<AxisView> {
        let mut out = Vec::with_capacity(AXIS_COUNT);
        let view = |i: usize, value: f32, confidence: f32| AxisView { name: AXES[i].0, hint: AXES[i].1, value, confidence };
        for i in 0..ACC_N {
            out.push(view(i, clamp01(self.acc[i].value), self.acc[i].confidence()));
        }
        let (range_v, range_c) = match self.range() {
            Some((lo, hi)) => (clamp01((hi - lo) as f32 / 24.0), 1.0 - (-(self.mapped_count() as f32) / 6.0).exp()),
            None => (0.0, 0.0),
        };
        out.push(view(5, range_v, range_c));
        out.push(view(6, clamp01(self.habit_days(today) as f32 / 10.0), 1.0));
        out
    }

    /// (strongest, weakest) axis among those with enough evidence.
    pub fn highlights(&self, today: i64) -> Option<(usize, usize)> {
        let axes = self.axes(today);
        let known: Vec<(usize, &AxisView)> = axes.iter().enumerate().filter(|(_, a)| a.confidence >= 0.3).collect();
        if known.len() < 3 {
            return None;
        }
        let best = known.iter().max_by(|a, b| a.1.value.partial_cmp(&b.1.value).unwrap())?.0;
        let worst = known.iter().min_by(|a, b| a.1.value.partial_cmp(&b.1.value).unwrap())?.0;
        (best != worst).then_some((best, worst))
    }

    /// Total seconds of voiced practice ever recorded in the kept days.
    pub fn total_secs(&self) -> f32 {
        self.days.iter().map(|d| d.1).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::cracks::Crack;
    use crate::dsp::undertone::{Undertone, SPEC_N};
    use crate::session::Goal;

    fn tgt() -> Target {
        Target::new(Goal::Feminine, (0.0, 0.0))
    }

    fn voiced(hz: f32) -> LiveFrame {
        LiveFrame { hz: Some(hz), ..Default::default() }
    }

    fn feed(p: &mut Profile, f: &LiveFrame, secs: f32) {
        let t = tgt();
        for _ in 0..(secs / 0.033) as usize {
            p.observe(f, 0.033, &t, 100);
        }
    }

    #[test]
    fn pitch_axis_follows_range() {
        let mut p = Profile::new();
        feed(&mut p, &voiced(200.0), 20.0);
        let a = p.axes(100);
        assert!(a[0].value > 0.95, "{}", a[0].value);
        feed(&mut p, &voiced(110.0), 20.0);
        let a = p.axes(100);
        assert!(a[0].value < 0.7 && a[0].value > 0.3, "{}", a[0].value);
    }

    #[test]
    fn builds_gradually() {
        let mut p = Profile::new();
        let c0 = p.axes(100)[0].confidence;
        feed(&mut p, &voiced(200.0), 5.0);
        let c1 = p.axes(100)[0].confidence;
        feed(&mut p, &voiced(200.0), 90.0);
        let c2 = p.axes(100)[0].confidence;
        assert!(c0 == 0.0 && c1 > 0.0 && c1 < 0.2 && c2 > 0.5, "{c0} {c1} {c2}");
    }

    #[test]
    fn cracks_pull_smooth_breaks_down() {
        let mut p = Profile::new();
        feed(&mut p, &voiced(200.0), 60.0);
        let before = p.axes(100)[3].value;
        let crack = LiveFrame { hz: Some(200.0), crack: Some(Crack { t: 1.0, from_hz: 400.0, to_hz: 200.0 }), ..Default::default() };
        p.observe(&crack, 0.033, &tgt(), 100);
        p.observe(&crack, 0.033, &tgt(), 100);
        let after = p.axes(100)[3].value;
        assert!(after < before - 0.1, "{before} {after}");
        // the crack is recorded on its low note
        let low = (hz_to_midi(200.0).round() as i32 - NOTE_MIN) as usize;
        assert_eq!(p.notes[low].cracks, 2.0);
    }

    #[test]
    fn steadiness_rewards_flat_pitch() {
        let mut flat = Profile::new();
        feed(&mut flat, &voiced(210.0), 10.0);
        let mut wobbly = Profile::new();
        let t = tgt();
        for i in 0..300 {
            // a slow +-4 semitone wobble
            let st = 4.0 * (i as f32 * 0.033 * std::f32::consts::TAU).sin();
            let hz = 210.0 * 2f32.powf(st / 12.0);
            wobbly.observe(&voiced(hz), 0.033, &t, 100);
        }
        assert!(flat.axes(100)[1].value > 0.95);
        assert!(wobbly.axes(100)[1].value < 0.5, "{}", wobbly.axes(100)[1].value);
    }

    #[test]
    fn clean_tone_penalises_fry_and_roughness() {
        let mut p = Profile::new();
        let clean = LiveFrame {
            hz: Some(200.0),
            under: Some(Undertone { airy: 0.0, fry: 0.0, rough: Some(0.0), spectrum: [0.0; SPEC_N], f0: 200.0 }),
            ..Default::default()
        };
        feed(&mut p, &clean, 20.0);
        assert!(p.axes(100)[4].value > 0.95);
        let rough = LiveFrame {
            hz: Some(200.0),
            under: Some(Undertone { airy: 0.0, fry: 0.0, rough: Some(1.0), spectrum: [0.0; SPEC_N], f0: 200.0 }),
            ..Default::default()
        };
        feed(&mut p, &rough, 20.0);
        assert!(p.axes(100)[4].value < 0.7);
    }

    #[test]
    fn range_maps_held_notes_only() {
        let mut p = Profile::new();
        // glide across C3 (130.8) .. G4 (392), each note held, plus one stray frame far above
        let t = tgt();
        for m in 48..=67 {
            let hz = crate::dsp::notes::midi_to_hz(m as f32);
            for _ in 0..20 {
                p.observe(&voiced(hz), 0.033, &t, 100);
            }
        }
        p.observe(&voiced(900.0), 0.033, &t, 100);
        let (lo, hi) = p.range().unwrap();
        assert_eq!((lo, hi), (48, 67));
        let a = p.axes(100);
        assert!((a[5].value - 19.0 / 24.0).abs() < 0.01);
        let (cl, ch) = p.comfort_zone().unwrap();
        assert!(cl >= 48 && ch <= 67 && ch - cl < 19);
    }

    #[test]
    fn habit_counts_days_with_enough_practice() {
        let mut p = Profile::new();
        let t = tgt();
        for day in [95i64, 96, 98] {
            for _ in 0..4000 {
                p.observe(&voiced(200.0), 0.033, &t, day);
            }
        }
        // a short day does not count
        for _ in 0..100 {
            p.observe(&voiced(200.0), 0.033, &t, 99);
        }
        assert_eq!(p.habit_days(100), 3);
        assert!((p.axes(100)[6].value - 0.3).abs() < 1e-5);
    }

    #[test]
    fn highlights_pick_strongest_and_weakest() {
        let mut p = Profile::new();
        feed(&mut p, &voiced(200.0), 120.0);
        // pitch is great, steadiness decent, smooth breaks great. Add rough tone to make clean the weakest.
        let rough = LiveFrame {
            hz: Some(200.0),
            under: Some(Undertone { airy: 1.0, fry: 1.0, rough: Some(1.0), spectrum: [0.0; SPEC_N], f0: 200.0 }),
            ..Default::default()
        };
        feed(&mut p, &rough, 120.0);
        let (best, worst) = p.highlights(100).unwrap();
        assert_eq!(worst, 4);
        assert_ne!(best, 4);
    }

    #[test]
    fn touch_and_resets() {
        let mut p = Profile::new();
        for m in 50..=60 {
            p.touch(m);
        }
        p.touch(500); // out of range is ignored
        assert_eq!(p.range(), Some((50, 60)));
        feed(&mut p, &voiced(200.0), 10.0);
        p.reset_web();
        assert_eq!(p.acc[0].weight, 0.0);
        assert!(p.days.is_empty());
        assert_eq!(p.range(), Some((50, 60)));
        p.reset_range();
        assert!(p.range().is_none());
    }

    #[test]
    fn glides_only_touch_the_range_map() {
        let mut p = Profile::new();
        let t = tgt();
        for _ in 0..200 {
            p.observe_with(&voiced(150.0), 0.033, &t, 100, false);
        }
        assert_eq!(p.acc[0].weight, 0.0);
        assert_eq!(p.acc[1].weight, 0.0);
        let i = (hz_to_midi(150.0).round() as i32 - NOTE_MIN) as usize;
        assert!(p.notes[i].secs > 6.0);
    }

    #[cfg(not(nodeps))]
    #[test]
    fn survives_a_save_and_load() {
        let mut p = Profile::new();
        feed(&mut p, &voiced(200.0), 5.0);
        let s = serde_json::to_string(&p).unwrap();
        let mut q: Profile = serde_json::from_str(&s).unwrap();
        q.prepare();
        assert_eq!(q.notes.len(), p.notes.len());
        assert!((q.acc[0].value - p.acc[0].value).abs() < 1e-6);
    }
}
