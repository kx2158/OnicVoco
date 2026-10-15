//! Session data, statistics, rule-based feedback and the passaggio finder.
//! All numbers are computed here. The optional AI coach only rephrases them.

use crate::dsp::cracks::Crack;
use crate::dsp::notes::{note_name, semitones};
use crate::dsp::register::Register;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Feminine,
    Androgynous,
    Masculine,
    Custom,
}

impl Goal {
    pub const ALL: [Goal; 4] = [Goal::Feminine, Goal::Androgynous, Goal::Masculine, Goal::Custom];
    pub fn label(self) -> &'static str {
        match self {
            Goal::Feminine => "Feminine",
            Goal::Androgynous => "Androgynous",
            Goal::Masculine => "Masculine",
            Goal::Custom => "Custom",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Goal::Feminine => "feminine",
            Goal::Androgynous => "androgynous",
            Goal::Masculine => "masculine",
            Goal::Custom => "custom",
        }
    }
    pub fn from_key(k: &str) -> Goal {
        match k {
            "androgynous" => Goal::Androgynous,
            "masculine" => Goal::Masculine,
            "custom" => Goal::Custom,
            _ => Goal::Feminine,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Target {
    pub goal: Goal,
    pub min_hz: f32,
    pub max_hz: f32,
    /// Resonance goal zone on the 0 (darker) .. 1 (brighter) scale.
    pub res: Option<(f32, f32, &'static str)>,
}

impl Target {
    pub fn new(goal: Goal, custom: (f32, f32)) -> Self {
        match goal {
            Goal::Feminine => Target { goal, min_hz: 165.0, max_hz: 255.0, res: Some((0.6, 1.0, "brighter")) },
            Goal::Androgynous => Target { goal, min_hz: 145.0, max_hz: 175.0, res: Some((0.4, 0.65, "balanced")) },
            Goal::Masculine => Target { goal, min_hz: 85.0, max_hz: 155.0, res: Some((0.0, 0.4, "darker")) },
            Goal::Custom => Target { goal, min_hz: custom.0.min(custom.1), max_hz: custom.0.max(custom.1), res: None },
        }
    }
    pub fn label(&self) -> &'static str {
        self.goal.label()
    }
    pub fn contains(&self, hz: f32) -> bool {
        hz >= self.min_hz && hz <= self.max_hz
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub t: f32,
    /// Median-smoothed pitch, used for display and stats.
    pub hz: Option<f32>,
    pub res: Option<f32>,
    pub h1h2: Option<f32>,
    pub register: Option<Register>,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub enough: bool,
    pub voiced_s: f32,
    pub median_hz: f32,
    pub low_hz: f32,
    pub high_hz: f32,
    pub range_st: f32,
    pub in_target: f32,
    pub below: f32,
    pub above: f32,
    /// Median absolute deviation from the median pitch, in semitones.
    pub spread: f32,
    pub resonance: Option<f32>,
    pub res_drift: Option<f32>,
    /// Share of voiced time per register (chest, mix, head).
    pub registers: Option<(f32, f32, f32)>,
    pub cracks: usize,
    pub crack_hz: Option<f32>,
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }
    let idx = ((sorted.len() - 1) as f32 * p).clamp(0.0, (sorted.len() - 1) as f32);
    let (lo, hi) = (idx.floor() as usize, idx.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo as f32)
}

fn sorted(mut v: Vec<f32>) -> Vec<f32> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

pub fn compute_stats(frames: &[Frame], cracks: &[Crack], target: &Target) -> Stats {
    let duration = match (frames.first(), frames.last()) {
        (Some(a), Some(b)) => b.t - a.t,
        _ => 0.0,
    };
    let hzs = sorted(frames.iter().filter_map(|f| f.hz).collect());
    let voiced_s = if frames.is_empty() { 0.0 } else { duration * hzs.len() as f32 / frames.len() as f32 };
    if hzs.len() < 5 {
        return Stats { voiced_s, ..Default::default() };
    }
    let n = hzs.len() as f32;
    let median = percentile(&hzs, 0.5);
    let low = percentile(&hzs, 0.05);
    let high = percentile(&hzs, 0.95);
    let devs = sorted(hzs.iter().map(|&h| semitones(median, h).abs()).collect());

    let res: Vec<f32> = frames.iter().filter_map(|f| f.res).collect();
    let resonance = (res.len() >= 5).then(|| percentile(&sorted(res.clone()), 0.5));
    let res_drift = (res.len() >= 20).then(|| {
        let half = res.len() / 2;
        let avg = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
        avg(&res[half..]) - avg(&res[..half])
    });

    let regs: Vec<Register> = frames.iter().filter(|f| f.hz.is_some()).filter_map(|f| f.register).collect();
    let registers = (regs.len() >= 5).then(|| {
        let c = |r| regs.iter().filter(|&&x| x == r).count() as f32 / regs.len() as f32;
        (c(Register::Chest), c(Register::Mix), c(Register::Head))
    });

    let crack_hz = (!cracks.is_empty())
        .then(|| percentile(&sorted(cracks.iter().map(|c| c.from_hz.min(c.to_hz)).collect()), 0.5));

    Stats {
        enough: true,
        voiced_s,
        median_hz: median,
        low_hz: low,
        high_hz: high,
        range_st: semitones(low, high),
        in_target: hzs.iter().filter(|&&h| target.contains(h)).count() as f32 / n,
        below: hzs.iter().filter(|&&h| h < target.min_hz).count() as f32 / n,
        above: hzs.iter().filter(|&&h| h > target.max_hz).count() as f32 / n,
        spread: percentile(&devs, 0.5),
        resonance,
        res_drift,
        registers,
        cracks: cracks.len(),
        crack_hz,
    }
}

fn pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round())
}

pub fn resonance_word(r: f32) -> &'static str {
    if r < 0.35 {
        "darker"
    } else if r < 0.6 {
        "balanced"
    } else {
        "brighter"
    }
}

/// Short rule-based tips. Always available, no model needed.
pub fn basic_feedback(s: &Stats, t: &Target) -> Vec<String> {
    if !s.enough {
        return vec!["Not enough voiced sound to analyse yet. Try speaking or reading aloud for at least 10 seconds.".into()];
    }
    let mut tips = Vec::new();
    let goal = t.label().to_lowercase();
    if s.in_target >= 0.7 {
        tips.push(format!("Lovely - {} of your voiced time sat in your {goal} pitch range.", pct(s.in_target)));
    } else if s.below > s.above {
        tips.push(format!(
            "{} of the time you dipped below {} Hz ({}). Try anchoring your sentences a little higher, especially at the ends of phrases.",
            pct(s.below),
            t.min_hz.round(),
            note_name(t.min_hz)
        ));
    } else {
        tips.push(format!(
            "{} of the time you went above {} Hz ({}). You can relax down a bit - a strained high pitch reads less natural than a comfortable one.",
            pct(s.above),
            t.max_hz.round(),
            note_name(t.max_hz)
        ));
    }

    if s.spread < 0.8 {
        tips.push("Your pitch was quite flat. Let your intonation rise and fall more, like you're telling a story you're excited about.".into());
    } else if s.spread > 3.5 {
        tips.push("Your pitch jumped around a lot. Great for expressiveness, but if it felt unsteady, try slower, smoother phrases.".into());
    } else {
        tips.push("Your intonation had a healthy, natural amount of movement.".into());
    }

    if let (Some(r), Some((lo, hi, word))) = (s.resonance, t.res) {
        if r < lo {
            tips.push("Resonance read darker than your goal. Big-dog/small-dog panting or a gentle smile shape on \"ee\" can help brighten it.".into());
        } else if r > hi {
            tips.push("Resonance read brighter than your goal. Try a more open, relaxed throat - think of the start of a yawn.".into());
        } else {
            tips.push(format!("Resonance landed in your {word} target zone. Nice!"));
        }
        if let Some(d) = s.res_drift {
            if d < -0.12 {
                tips.push("Your resonance got darker as the session went on. That often happens when you get tired - short breaks help.".into());
            } else if d > 0.12 {
                tips.push("Your resonance got brighter as you warmed up. Good sign!".into());
            }
        }
    }

    if let Some((chest, _mix, head)) = s.registers {
        if head > 0.5 && t.goal != Goal::Masculine {
            tips.push(format!(
                "You spent {} of the time in a light head/falsetto quality. That can sound breathy in everyday speech - try adding a little more \"buzz\" (a light mix) so it carries.",
                pct(head)
            ));
        } else if chest > 0.8 && t.goal == Goal::Feminine {
            tips.push("You stayed mostly in heavy chest voice. Lightening your vocal weight (speaking as if you're talking to a small child, softer and thinner) helps a lot with feminine voice.".into());
        }
    }

    if s.cracks > 0 {
        let where_ = s.crack_hz.map(|h| format!(" around {} ({} Hz)", note_name(h), h.round())).unwrap_or_default();
        tips.push(format!(
            "{} voice crack{} detected{where_}. That's usually your passaggio. Lower the volume as you approach it and lighten up instead of pushing - see Learn > Voice cracks.",
            s.cracks,
            if s.cracks == 1 { "" } else { "s" }
        ));
    }

    tips.push("Reminder: rest if your throat feels tight or scratchy. Voice training should never hurt.".into());
    tips
}

/// Compact, factual summary handed to the optional AI coach.
pub fn coach_prompt(s: &Stats, t: &Target) -> String {
    let mut lines = vec![
        format!("Goal: {} voice, pitch range {}-{} Hz.", t.label(), t.min_hz.round(), t.max_hz.round()),
        format!("Voiced speaking time: {} seconds.", s.voiced_s.round()),
        format!("Median pitch: {} Hz ({}).", s.median_hz.round(), note_name(s.median_hz)),
        format!("Typical range: {} to {} ({:.1} semitones).", note_name(s.low_hz), note_name(s.high_hz), s.range_st),
        format!("Time inside goal range: {}. Below: {}. Above: {}.", pct(s.in_target), pct(s.below), pct(s.above)),
        format!(
            "Intonation movement: {}.",
            if s.spread < 0.8 { "flat" } else if s.spread > 3.5 { "very jumpy" } else { "natural" }
        ),
    ];
    if let Some(r) = s.resonance {
        lines.push(format!("Resonance: {} (score {:.2}, 0=dark, 1=bright).", resonance_word(r), r));
        if let Some((_, _, w)) = t.res {
            lines.push(format!("Resonance goal: {w}."));
        }
    }
    if let Some((c, m, h)) = s.registers {
        lines.push(format!("Register estimate: chest {}, mix {}, head/falsetto {}.", pct(c), pct(m), pct(h)));
    }
    lines.push(format!("Voice cracks detected: {}.", s.cracks));
    lines.join("\n")
}

pub const COACH_SYSTEM_PROMPT: &str = "You are a warm, encouraging voice training coach. You receive measured statistics from a practice session. \
Write 3 to 5 short sentences of feedback in plain, friendly language: one thing that went well, one or two things to work on, and one concrete exercise to try next. \
Only use the numbers given. Do not invent measurements. Never comment on the person's identity or whether they \"pass\". Do not use em dashes.";

#[derive(Debug, Clone, PartialEq)]
pub enum Passaggio {
    NotEnough,
    /// The voice broke or switched register around this pitch.
    Found { hz: f32, via_crack: bool },
    /// Glide covered this range without a clear break.
    Smooth { low_hz: f32, high_hz: f32 },
}

/// Analyse a low-to-high siren to find where the voice changes gear.
pub fn find_passaggio(frames: &[Frame], cracks: &[Crack]) -> Passaggio {
    let voiced: Vec<&Frame> = frames.iter().filter(|f| f.hz.is_some()).collect();
    if voiced.len() < 30 {
        return Passaggio::NotEnough;
    }
    if let Some(c) = cracks.iter().find(|c| c.upward()) {
        return Passaggio::Found { hz: c.from_hz, via_crack: true };
    }
    // Sustained chest, then a sustained switch to mix/head.
    const RUN: usize = 8; // ~0.25 s at 30 frames per second
    let mut chest_run = 0usize;
    let mut seen_chest = false;
    for (i, f) in voiced.iter().enumerate() {
        match f.register {
            Some(Register::Chest) => {
                chest_run += 1;
                if chest_run >= RUN {
                    seen_chest = true;
                }
            }
            Some(_) if seen_chest => {
                let window = &voiced[i..(i + RUN).min(voiced.len())];
                if window.len() == RUN && window.iter().all(|w| w.register.is_some_and(|r| r != Register::Chest)) {
                    return Passaggio::Found { hz: f.hz.unwrap(), via_crack: false };
                }
                chest_run = 0;
            }
            _ => chest_run = 0,
        }
    }
    let hzs = sorted(voiced.iter().filter_map(|f| f.hz).collect());
    Passaggio::Smooth { low_hz: percentile(&hzs, 0.05), high_hz: percentile(&hzs, 0.95) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(i: usize, hz: f32, reg: Register) -> Frame {
        Frame { t: i as f32 * 0.033, hz: Some(hz), res: Some(0.5), h1h2: None, register: Some(reg) }
    }

    #[test]
    fn stats_and_feedback() {
        let t = Target::new(Goal::Feminine, (160.0, 220.0));
        let frames: Vec<Frame> = (0..300)
            .map(|i| {
                let mut f = frame(i, 190.0 + 20.0 * (i as f32 / 10.0).sin(), Register::Mix);
                if i % 4 == 0 {
                    f.hz = None;
                }
                f
            })
            .collect();
        let s = compute_stats(&frames, &[], &t);
        assert!(s.enough && s.in_target > 0.95);
        let tips = basic_feedback(&s, &t);
        assert!(tips.iter().any(|x| x.contains("darker")));
        let p = coach_prompt(&s, &t);
        assert!(p.contains("Median pitch"));
        assert!(!tips.join(" ").contains('—') && !p.contains('—'));
    }

    #[test]
    fn passaggio_from_register_shift() {
        let mut frames = Vec::new();
        for i in 0..40 {
            frames.push(frame(i, 150.0 + i as f32 * 4.0, Register::Chest));
        }
        for i in 40..80 {
            frames.push(frame(i, 310.0 + (i - 40) as f32 * 3.0, Register::Head));
        }
        match find_passaggio(&frames, &[]) {
            Passaggio::Found { hz, via_crack } => {
                assert!(!via_crack);
                assert!((hz - 310.0).abs() < 1.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn passaggio_smooth() {
        let frames: Vec<Frame> = (0..60).map(|i| frame(i, 150.0 + i as f32 * 3.0, Register::Mix)).collect();
        assert!(matches!(find_passaggio(&frames, &[]), Passaggio::Smooth { .. }));
    }
}
