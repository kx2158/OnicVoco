use crate::ai::{self, AiCoach, Msg};
use crate::analyzer::{Analyzer, LiveFrame, FRAME};
use crate::audio::{self, AudioInput, Player};
use crate::content::{GUIDES, PASSAGES, QUICK_PHRASES};
use crate::dsp::cracks::Crack;
use crate::dsp::notes::{format_cents, hz_to_midi, hz_to_note, midi_name, note_name};
use crate::dsp::undertone;
use crate::profile::{self, Profile, NOTE_MIN};
use crate::session::{self, find_passaggio, resonance_word, Frame, Goal, Passaggio, Stats, Target};
use crate::storage::{self, HistoryEntry, Settings};
use crate::theme::{self, bold, Pal};
use crate::widgets::{self, card, chip, ghost_button, muted, primary_button, segmented, title, Mood, RangeOpts, View};
use chrono::Datelike;
use eframe::egui::{self, vec2, Align, Key, Layout, RichText, Ui};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Practice,
    Read,
    Learn,
    Range,
    Progress,
    Settings,
}

const TABS: [(Tab, &str); 6] = [
    (Tab::Practice, "Practice"),
    (Tab::Read, "Read"),
    (Tab::Learn, "Learn"),
    (Tab::Range, "Range"),
    (Tab::Progress, "Progress"),
    (Tab::Settings, "Settings"),
];

const LIVE_WINDOW: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Practice,
    Passaggio,
}

/// A session being recorded. Times are relative to its start.
struct Running {
    mode: Mode,
    t0: f32,
    frames: Vec<Frame>,
    points: Vec<(f32, Option<f32>)>,
    cracks: Vec<Crack>,
}

/// A guided "map my range" glide.
struct MapRun {
    cand: Option<(i32, f32)>,
    low: Option<i32>,
    high: Option<i32>,
}

fn today() -> i64 {
    chrono::Local::now().date_naive().num_days_from_ce() as i64
}

struct Summary {
    stats: Stats,
    tips: Vec<String>,
    target: Target,
    points: Vec<(f32, Option<f32>)>,
    cracks: Vec<Crack>,
    duration: f32,
    recording: Option<(Arc<Vec<f32>>, u32)>,
    ai_text: String,
}

enum AiState {
    Idle,
    Downloading(f32, String),
    Thinking,
}

pub struct App {
    pal: Pal,
    applied_dark: Option<bool>,
    last_theme_check: Instant,
    settings: Settings,
    history: Vec<HistoryEntry>,
    tab: Tab,
    tab_changed: Instant,
    devices: Vec<String>,

    // live microphone
    audio: Option<AudioInput>,
    audio_device: Option<Option<String>>,
    last_open_attempt: Option<Instant>,
    clock: Instant,
    analyzer: Analyzer,
    buf: Vec<f32>,
    last_analysis: Instant,
    live: LiveFrame,
    peak_hold: (f32, Instant),
    graph: VecDeque<(f32, Option<f32>)>,
    graph_cracks: Vec<Crack>,
    graph_whole: bool,
    in_range_since: Option<Instant>,
    last_crack_at: Option<Instant>,

    // sessions
    running: Option<Running>,
    summary: Option<Summary>,
    player: Option<Player>,
    play_pos: f32,
    passaggio_result: Option<Passaggio>,
    toast: Option<(String, Instant)>,
    error: Option<String>,

    // AI coach
    ai: AiCoach,
    ai_state: AiState,
    ai_installed: bool,
    ai_message: Option<String>,

    // voice profile: range map + skill web
    profile: Profile,
    profile_saved: Instant,
    map_run: Option<MapRun>,
    map_result: Option<(i32, i32)>,
    confirm_web: bool,
    confirm_range: bool,

    guide: usize,
    phrase: usize,
    confirm_clear: bool,
}

fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let settings = storage::load_settings();
        let pal = theme::resolve(&settings.theme);
        theme::install(&cc.egui_ctx, &pal);
        let now = Instant::now();
        App {
            pal,
            applied_dark: Some(pal.dark),
            last_theme_check: now,
            history: storage::load_history(),
            tab: Tab::Practice,
            tab_changed: now,
            devices: audio::list_devices(),
            audio: None,
            audio_device: None,
            last_open_attempt: None,
            clock: now,
            analyzer: Analyzer::new(settings.gate_db),
            buf: Vec::with_capacity(FRAME),
            last_analysis: now,
            live: LiveFrame { peak_db: -100.0, db: -100.0, ..Default::default() },
            peak_hold: (-100.0, now),
            graph: VecDeque::new(),
            graph_cracks: Vec::new(),
            graph_whole: false,
            in_range_since: None,
            last_crack_at: None,
            running: None,
            summary: None,
            player: None,
            play_pos: 0.0,
            passaggio_result: None,
            toast: None,
            error: None,
            ai: AiCoach::spawn(cc.egui_ctx.clone()),
            ai_state: AiState::Idle,
            ai_installed: ai::installed(),
            ai_message: None,
            profile: storage::load_profile(),
            profile_saved: now,
            map_run: None,
            map_result: None,
            confirm_web: false,
            confirm_range: false,
            guide: 0,
            phrase: 0,
            confirm_clear: false,
            settings,
        }
    }

    fn target(&self) -> Target {
        Target::new(Goal::from_key(&self.settings.goal), (self.settings.custom_min, self.settings.custom_max))
    }

    fn save(&self) {
        storage::save_settings(&self.settings);
    }

    fn now_t(&self) -> f32 {
        self.clock.elapsed().as_secs_f32()
    }

    // ---------- theme ----------

    fn refresh_theme(&mut self, ctx: &egui::Context, force: bool) {
        if force || self.last_theme_check.elapsed() > Duration::from_secs(2) {
            self.last_theme_check = Instant::now();
            self.pal = theme::resolve(&self.settings.theme);
        }
        if force || self.applied_dark != Some(self.pal.dark) || ctx.style().visuals.panel_fill != self.pal.bg {
            theme::install(ctx, &self.pal);
            self.applied_dark = Some(self.pal.dark);
        }
    }

    // ---------- microphone ----------

    fn want_mic(&self) -> bool {
        self.running.is_some() || (self.settings.live_mic && matches!(self.tab, Tab::Practice | Tab::Read | Tab::Range))
    }

    fn open_mic(&mut self) -> Result<(), String> {
        self.last_open_attempt = Some(Instant::now());
        let a = AudioInput::start(self.settings.mic.as_deref())?;
        self.audio = Some(a);
        self.audio_device = Some(self.settings.mic.clone());
        self.analyzer = Analyzer::new(self.settings.gate_db);
        self.clock = Instant::now();
        self.graph.clear();
        self.graph_cracks.clear();
        Ok(())
    }

    fn close_mic(&mut self) {
        self.audio = None;
        self.audio_device = None;
        self.live = LiveFrame { peak_db: -100.0, db: -100.0, ..Default::default() };
        self.in_range_since = None;
    }

    /// Opens or closes the mic to match what the current screen needs.
    fn manage_mic(&mut self) {
        let want = self.want_mic();
        if self.audio.is_some() && (!want || self.audio_device.as_ref() != Some(&self.settings.mic)) && self.running.is_none() {
            self.close_mic();
        }
        if want && self.audio.is_none() {
            let may_retry = self.last_open_attempt.map_or(true, |t| t.elapsed() > Duration::from_secs(4));
            if may_retry {
                match self.open_mic() {
                    Ok(()) => self.error = None,
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }

    fn tick_audio(&mut self, ctx: &egui::Context) {
        let Some(audio) = self.audio.as_ref() else { return };
        ctx.request_repaint_after(Duration::from_millis(16));
        if let Some(e) = audio.error() {
            self.error = Some(format!("Microphone stopped: {e}"));
        }
        if self.last_analysis.elapsed() < Duration::from_millis(33) {
            return;
        }
        self.last_analysis = Instant::now();
        if !audio.latest(FRAME, &mut self.buf) {
            return;
        }
        let t = self.clock.elapsed().as_secs_f32();
        let f = self.analyzer.analyze(&self.buf, audio.sample_rate, t);

        // peak hold: holds for a second, then falls back
        let (held, at) = self.peak_hold;
        if f.peak_db >= held || at.elapsed() > Duration::from_millis(1200) {
            self.peak_hold = (f.peak_db, Instant::now());
        }

        let target = self.target();
        let dt = (t - self.live.t).clamp(0.0, 0.1);
        let web = self.map_run.is_none() && self.running.as_ref().map_or(true, |r| r.mode == Mode::Practice);
        self.profile.observe_with(&f, dt, &target, today(), web);
        let mut touched = None;
        if let Some(m) = self.map_run.as_mut() {
            match f.hz.map(|h| hz_to_midi(h).round() as i32) {
                Some(n) => {
                    // a note counts once it has been steady for a few frames
                    let held = match m.cand {
                        Some((c, s)) if c == n => s + dt,
                        _ => dt,
                    };
                    m.cand = Some((n, held));
                    if held >= 0.1 {
                        m.low = Some(m.low.map_or(n, |l| l.min(n)));
                        m.high = Some(m.high.map_or(n, |h| h.max(n)));
                        touched = Some(n);
                    }
                }
                None => m.cand = None,
            }
        }
        if let Some(n) = touched {
            self.profile.touch(n);
        }
        match f.hz {
            Some(h) if target.contains(h) => {
                self.in_range_since.get_or_insert_with(Instant::now);
            }
            Some(_) => self.in_range_since = None,
            None => {}
        }

        if let Some(run) = self.running.as_mut() {
            let rt = t - run.t0;
            run.frames.push(Frame { t: rt, hz: f.hz, res: f.res, h1h2: f.h1h2, register: f.register });
            run.points.push((rt, f.hz));
            if let Some(c) = f.crack {
                run.cracks.push(Crack { t: c.t - run.t0, ..c });
            }
        }
        if let Some(c) = f.crack {
            self.graph_cracks.push(c);
            self.last_crack_at = Some(Instant::now());
            if self.running.as_ref().map_or(true, |r| r.mode == Mode::Practice) {
                let low = c.from_hz.min(c.to_hz);
                let dir = if c.upward() { "up into head voice" } else { "down into chest voice" };
                self.toast = Some((
                    format!("Crack at {} ({dir}). Get a little quieter and lighter as you approach this note.", note_name(low)),
                    Instant::now(),
                ));
            }
        }
        self.graph.push_back((t, f.hz));
        while self.graph.front().is_some_and(|(pt, _)| *pt < t - LIVE_WINDOW - 1.0) {
            self.graph.pop_front();
        }
        self.graph_cracks.retain(|c| c.t > t - LIVE_WINDOW - 1.0);
        self.live = f;
    }

    // ---------- sessions ----------

    fn start(&mut self, mode: Mode) {
        self.error = None;
        if self.audio.is_none() {
            if let Err(e) = self.open_mic() {
                self.error = Some(e);
                return;
            }
        }
        self.player = None;
        if mode == Mode::Passaggio {
            self.passaggio_result = None;
        }
        if let Some(a) = &self.audio {
            a.set_recording(self.settings.record && mode == Mode::Practice);
        }
        self.running = Some(Running { mode, t0: self.now_t(), frames: Vec::new(), points: Vec::new(), cracks: Vec::new() });
    }

    fn stop(&mut self) {
        let Some(run) = self.running.take() else { return };
        let recording = self.audio.as_ref().and_then(|a| {
            let r = a.take_recording();
            a.set_recording(false);
            r.map(|r| (Arc::new(r), a.sample_rate as u32))
        });
        match run.mode {
            Mode::Passaggio => {
                let result = find_passaggio(&run.frames, &run.cracks);
                if let Passaggio::Found { hz, .. } = result {
                    self.settings.passaggio_hz = Some(hz);
                    self.save();
                }
                self.passaggio_result = Some(result);
            }
            Mode::Practice => {
                let target = self.target();
                let stats = session::compute_stats(&run.frames, &run.cracks, &target);
                let tips = session::basic_feedback(&stats, &target);
                if stats.enough {
                    self.history.insert(
                        0,
                        HistoryEntry {
                            at: chrono::Local::now().timestamp(),
                            goal: target.label().into(),
                            median_hz: stats.median_hz.round(),
                            note: note_name(stats.median_hz),
                            in_target: stats.in_target,
                            resonance: stats.resonance,
                            seconds: stats.voiced_s.round(),
                            cracks: stats.cracks,
                            registers: stats.registers,
                        },
                    );
                    self.history.truncate(300);
                    storage::save_history(&self.history);
                }
                let duration = run.points.last().map(|p| p.0).unwrap_or(0.0);
                self.play_pos = 0.0;
                self.summary = Some(Summary {
                    stats,
                    tips,
                    target,
                    points: run.points,
                    cracks: run.cracks,
                    duration,
                    recording: recording.filter(|(r, _)| !r.is_empty()),
                    ai_text: String::new(),
                });
            }
        }
    }

    fn start_map(&mut self) {
        self.error = None;
        self.map_result = None;
        self.map_run = Some(MapRun { cand: None, low: None, high: None });
    }

    fn finish_map(&mut self) {
        if let Some(m) = self.map_run.take() {
            self.map_result = m.low.zip(m.high).filter(|(a, b)| b > a);
            self.save_profile();
        }
    }

    fn save_profile(&mut self) {
        storage::save_profile(&self.profile);
        self.profile.dirty = false;
        self.profile_saved = Instant::now();
    }

    fn toggle_play(&mut self) {
        if let Some(p) = self.player.take() {
            self.play_pos = p.position_secs();
            return;
        }
        let Some(s) = self.summary.as_ref() else { return };
        let Some((rec, sr)) = s.recording.clone() else { return };
        if self.play_pos >= s.duration - 0.05 {
            self.play_pos = 0.0;
        }
        match Player::start(rec, sr, self.play_pos) {
            Ok(p) => self.player = Some(p),
            Err(e) => self.error = Some(e),
        }
    }

    fn seek(&mut self, t: f32) {
        self.play_pos = t;
        if self.player.is_some() {
            self.player = None;
            self.toggle_play();
        }
    }

    fn poll_player(&mut self, ctx: &egui::Context) {
        if let Some(p) = &self.player {
            self.play_pos = p.position_secs();
            if p.finished() {
                self.player = None;
                self.play_pos = 0.0;
            }
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    fn poll_ai(&mut self) {
        while let Ok(m) = self.ai.rx.try_recv() {
            match m {
                Msg::Progress(f, s) => {
                    if matches!(self.ai_state, AiState::Downloading(..)) {
                        self.ai_state = AiState::Downloading(f, s);
                    } else {
                        self.ai_message = Some(s);
                    }
                }
                Msg::Installed => {
                    self.ai_state = AiState::Idle;
                    self.ai_installed = true;
                    self.settings.ai_enabled = true;
                    self.ai_message = Some("AI coach installed and ready.".into());
                    self.save();
                }
                Msg::Token(t) => {
                    self.ai_message = None;
                    if let Some(s) = self.summary.as_mut() {
                        s.ai_text.push_str(&t);
                    }
                }
                Msg::Done => self.ai_state = AiState::Idle,
                Msg::Error(e) => {
                    self.ai_state = AiState::Idle;
                    self.ai_message = Some(e);
                }
            }
        }
    }

    fn mood(&self) -> Mood {
        if self.audio.is_none() {
            Mood::Sleepy
        } else if self.last_crack_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1600)) {
            Mood::Oops
        } else if self.in_range_since.is_some_and(|t| t.elapsed() > Duration::from_millis(2500)) && self.live.hz.is_some() {
            Mood::Happy
        } else {
            Mood::Listening
        }
    }

    // ---------- top bar ----------

    fn top_bar(&mut self, ui: &mut Ui) {
        let p = self.pal;
        ui.horizontal(|ui| {
            let level = ((self.live.db + 60.0) / 45.0).clamp(0.0, 1.0);
            widgets::mascot(ui, &p, self.mood(), if self.live.hz.is_some() { level } else { 0.0 });
            ui.label(RichText::new("OnicVoco").family(bold()).size(23.0));
            ui.add_space(14.0);
            let idx = TABS.iter().position(|(t, _)| *t == self.tab).unwrap_or(0);
            let names: Vec<&str> = TABS.iter().map(|(_, n)| *n).collect();
            if let Some(i) = segmented(ui, &p, "tabs", &names, idx) {
                self.tab = TABS[i].0;
                self.tab_changed = Instant::now();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let icon = if p.dark { "☀" } else { "☾" };
                if ghost_button(ui, &p, icon).on_hover_text("Switch theme").clicked() {
                    self.settings.theme = if p.dark { "light".into() } else { "dark".into() };
                    self.save();
                    self.pal = theme::resolve(&self.settings.theme);
                }
                if let Some(run) = &self.running {
                    let label = if run.mode == Mode::Passaggio { "● finding passaggio" } else { "● REC" };
                    chip(ui, &p, &format!("{label} {}", mmss(self.now_t() - run.t0)), true);
                }
            });
        });
    }

    // ---------- shared pieces ----------

    fn start_stop_buttons(&mut self, ui: &mut Ui) {
        let p = self.pal;
        if self.running.is_none() {
            if primary_button(ui, &p, "●  Start recording", false).on_hover_text("Space").clicked() {
                self.start(Mode::Practice);
            }
        } else if primary_button(ui, &p, "■  Finish", true).on_hover_text("Space").clicked() {
            self.stop();
        }
    }

    fn status(&self, target: &Target) -> (&'static str, bool) {
        if self.audio.is_none() {
            return ("Mic off", false);
        }
        match self.live.hz {
            Some(h) if target.contains(h) => ("In your range", true),
            Some(h) if h < target.min_hz => ("A little low", false),
            Some(_) => ("A little high", false),
            None => ("Listening…", false),
        }
    }

    fn error_banner(&mut self, ui: &mut Ui) {
        if let Some(e) = self.error.clone() {
            let p = self.pal;
            card(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠").size(18.0));
                    ui.label(RichText::new(&e).family(bold()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ghost_button(ui, &p, "Dismiss").clicked() {
                            self.error = None;
                        }
                    });
                });
                muted(ui, &p, "If Windows blocked the microphone: Settings > Privacy & security > Microphone > allow desktop apps.");
            });
        }
    }

    fn mic_combo(&mut self, ui: &mut Ui) {
        let current = self.settings.mic.clone().unwrap_or_else(|| "Default microphone".into());
        ui.add_enabled_ui(self.running.is_none(), |ui| {
            egui::ComboBox::new("mic", "").selected_text(current).width(230.0).show_ui(ui, |ui| {
                let mut sel = self.settings.mic.clone();
                ui.selectable_value(&mut sel, None, "Default microphone");
                for d in &self.devices {
                    ui.selectable_value(&mut sel, Some(d.clone()), d);
                }
                if sel != self.settings.mic {
                    self.settings.mic = sel;
                    self.save();
                }
            });
        });
    }

    fn graph_card(&mut self, ui: &mut Ui, target: &Target, height: f32, with_toggle: bool) {
        let p = self.pal;
        let now = self.now_t();
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Pitch over time");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if with_toggle {
                        let idx = usize::from(self.graph_whole);
                        if let Some(i) = segmented(ui, &p, "graphview", &["Last 12 s", "Whole session"], idx) {
                            self.graph_whole = i == 1;
                        }
                    }
                    muted(ui, &p, "thick = in range · ◉ = crack");
                });
            });
            let show_whole = with_toggle && self.graph_whole;
            if show_whole {
                let (pts, cracks): (&[(f32, Option<f32>)], &[Crack]) = match (&self.running, &self.summary) {
                    (Some(r), _) => (&r.points, &r.cracks),
                    (None, Some(s)) => (&s.points, &s.cracks),
                    _ => (&[], &[]),
                };
                let end = pts.last().map(|p| p.0).unwrap_or(1.0).max(1.0);
                let live_now = self.running.as_ref().map(|r| now - r.t0);
                widgets::pitch_graph(ui, &p, pts, cracks, target, self.settings.passaggio_hz, height, View { start: 0.0, end }, live_now, None);
                if pts.is_empty() {
                    muted(ui, &p, "Start a recording and the whole session will fit here.");
                }
            } else {
                let pts: Vec<(f32, Option<f32>)> = self.graph.iter().cloned().collect();
                let end = if self.audio.is_some() { now } else { pts.last().map(|p| p.0).unwrap_or(LIVE_WINDOW) };
                let view = View { start: end - LIVE_WINDOW, end };
                widgets::pitch_graph(ui, &p, &pts, &self.graph_cracks, target, self.settings.passaggio_hz, height, view, Some(now), None);
            }
        });
    }

    // ---------- Practice ----------

    fn practice(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let target = self.target();
        self.error_banner(ui);

        card(ui, &p, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    muted(ui, &p, "Microphone");
                    self.mic_combo(ui);
                    if self.running.is_none() && ui.small_button("Refresh list").clicked() {
                        self.devices = audio::list_devices();
                    }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    muted(ui, &p, "Goal");
                    let goals: Vec<&str> = Goal::ALL.iter().map(|g| g.label()).collect();
                    let idx = Goal::ALL.iter().position(|g| *g == target.goal).unwrap_or(0);
                    if let Some(i) = segmented(ui, &p, "goal", &goals, idx) {
                        self.settings.goal = Goal::ALL[i].key().into();
                        self.save();
                    }
                    if target.goal == Goal::Custom {
                        ui.horizontal(|ui| {
                            let a = ui.add(egui::DragValue::new(&mut self.settings.custom_min).range(50.0..=700.0).suffix(" Hz"));
                            ui.label("to");
                            let b = ui.add(egui::DragValue::new(&mut self.settings.custom_max).range(50.0..=700.0).suffix(" Hz"));
                            if a.changed() || b.changed() {
                                self.save();
                            }
                        });
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    self.start_stop_buttons(ui);
                    if self.running.is_none() && ghost_button(ui, &p, "Find my passaggio").clicked() {
                        self.start(Mode::Passaggio);
                    }
                });
            });
        });

        if self.running.as_ref().is_some_and(|r| r.mode == Mode::Passaggio) {
            card(ui, &p, |ui| {
                title(ui, "Finding your passaggio");
                ui.label("On a quiet \"oo\" or \"ng\", glide slowly from your lowest comfortable note up to your highest. Don't push. When you reach the top, press Finish.");
                muted(ui, &p, "The app watches for the point where your voice flips or shifts register.");
            });
        }

        ui.columns(2, |cols| {
            self.live_card(&mut cols[0], &target);
            self.undertone_card(&mut cols[0]);
            self.register_card(&mut cols[1]);
            self.resonance_card(&mut cols[1], &target);
        });

        self.graph_card(ui, &target, 240.0, true);

        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Notes");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    muted(ui, &p, "scientific pitch notation · A4 = 440 Hz · shaded = goal · dot = passaggio");
                });
            });
            let cur = self.live.hz.map(|h| hz_to_note(h).midi);
            widgets::note_strip(ui, &p, cur, &target, self.settings.passaggio_hz);
        });

        self.passaggio_card(ui);
        self.summary_card(ui);
    }

    fn live_card(&mut self, ui: &mut Ui, target: &Target) {
        let p = self.pal;
        let (status, good) = self.status(target);
        let ctx = ui.ctx().clone();
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Pitch");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| chip(ui, &p, status, good));
            });
            ui.horizontal(|ui| {
                let voiced = self.live.hz.is_some();
                let shown = ctx.animate_value_with_time(egui::Id::new("hz-num"), self.live.hz.unwrap_or(0.0), 0.12);
                let alpha = ctx.animate_bool_with_time(egui::Id::new("hz-on"), voiced, 0.25);
                let (hz, note, cents) = match self.live.hz {
                    Some(h) => {
                        let n = hz_to_note(h);
                        (format!("{}", shown.round()), n.name, format_cents(n.cents))
                    }
                    None => ("--".into(), "--".into(), String::new()),
                };
                let c = widgets::lerp_color(p.faint, p.text, alpha.max(if self.audio.is_none() { 1.0 } else { 0.0 }));
                ui.label(RichText::new(hz).family(bold()).size(72.0).color(c));
                ui.label(RichText::new("Hz").size(18.0).color(p.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(cents).family(bold()).size(16.0).color(p.muted));
                    ui.label(RichText::new(note).family(bold()).size(40.0).color(c));
                });
            });
            let active = self.audio.is_some();
            widgets::peak_meter(ui, &p, self.live.peak_db, self.peak_hold.0, self.live.db, active);
            muted(
                ui,
                &p,
                format!(
                    "Goal: {} range, {}-{} Hz ({} to {})",
                    target.label().to_lowercase(),
                    target.min_hz.round(),
                    target.max_hz.round(),
                    note_name(target.min_hz),
                    note_name(target.max_hz)
                ),
            );
            if let Some((msg, at)) = &self.toast {
                let age = at.elapsed().as_secs_f32();
                if age < 5.0 {
                    let a = (age / 0.25).min(1.0) * ((5.0 - age) / 0.5).min(1.0);
                    egui::Frame::none()
                        .fill(widgets::fade(p.card2, a))
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(msg).size(13.5).color(widgets::fade(p.text, a)));
                        });
                    ctx.request_repaint();
                }
            }
        });
    }

    fn undertone_card(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let active = self.audio.is_some();
        let under = self.live.under;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Undertones");
                chip(ui, &p, "estimate", false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let (Some(u), true) = (under, active) {
                        if u.fry > 0.6 {
                            muted(ui, &p, format!("creak around {} Hz", u.f0.round()));
                        }
                    }
                });
            });
            widgets::undertone_monitor(ui, &p, under.as_ref(), active);
            let msg = match (&under, active) {
                (Some(u), true) => undertone::describe(u),
                _ => "Make a sound and the spectrum lights up.",
            };
            ui.label(RichText::new(msg).size(13.5));
            muted(
                ui,
                &p,
                "The bars show your sound around the pitch. Tall bars at \"half\" are sub-harmonics (a rough, rattly edge). Roughness needs a pitch above about 130 Hz to measure.",
            );
        });
    }

    fn register_card(&mut self, ui: &mut Ui) {
        let p = self.pal;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Register");
                chip(ui, &p, "estimate", false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let (Some(h), true) = (self.live.h1h2, self.live.hz.is_some()) {
                        muted(ui, &p, format!("H1-H2 {h:.1} dB"));
                    }
                });
            });
            widgets::register_meter(ui, &p, if self.live.hz.is_some() { self.live.register } else { None });
            muted(ui, &p, "Chest is buzzy, head/falsetto is light and airy, mix sits in between. A big jump between them is a crack.");
        });
    }

    fn resonance_card(&mut self, ui: &mut Ui, target: &Target) {
        let p = self.pal;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Resonance");
                chip(ui, &p, "estimate", false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let (Some(f), true) = (self.live.formants, self.live.hz.is_some()) {
                        muted(ui, &p, format!("F1 {:.0} · F2 {:.0} Hz", f.f1, f.f2));
                    }
                });
            });
            let v = if self.live.hz.is_some() { self.live.res } else { None };
            widgets::resonance_meter(ui, &p, v, target.res.map(|(a, b, _)| (a, b)));
            muted(ui, &p, "Resonance shifts with every vowel, so watch the trend over a few sentences.");
        });
    }

    fn passaggio_card(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let Some(res) = self.passaggio_result.clone() else { return };
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Your passaggio");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, &p, "Close").clicked() {
                        self.passaggio_result = None;
                    }
                });
            });
            match res {
                Passaggio::Found { hz, via_crack } => {
                    ui.label(RichText::new(format!("around {} ({} Hz)", note_name(hz), hz.round())).family(bold()).size(30.0));
                    ui.label(if via_crack {
                        "Your voice cracked here on the way up. This is the zone to practise: quiet, light sirens across it."
                    } else {
                        "Your voice shifted from chest toward head voice here. It's now marked on the graph and note strip."
                    });
                }
                Passaggio::Smooth { low_hz, high_hz } => {
                    ui.label(RichText::new("No clear break found").family(bold()).size(24.0));
                    ui.label(format!(
                        "You glided from {} to {} without an obvious flip. That's a smooth transition! Try going higher, or a little louder, to find it.",
                        note_name(low_hz),
                        note_name(high_hz)
                    ));
                }
                Passaggio::NotEnough => {
                    ui.label("Not enough voiced sound. Try a slower, longer glide (about 5-10 seconds).");
                }
            }
        });
    }

    fn summary_card(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let ai_ready = self.ai_installed && self.settings.ai_enabled;
        let thinking = matches!(self.ai_state, AiState::Thinking);
        let playing = self.player.is_some();
        let play_pos = self.play_pos;
        let mut ask_ai = None;
        let mut close = false;
        let mut toggle = false;
        let mut seek = None;
        let mut save = false;
        let Some(s) = self.summary.as_ref() else { return };
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Session summary");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, &p, "Close").clicked() {
                        close = true;
                    }
                    if s.recording.is_some() && ghost_button(ui, &p, "Save recording…").clicked() {
                        save = true;
                    }
                });
            });

            // The whole recording, fitted to the width, with a playhead.
            let has_audio = s.recording.is_some();
            let out = widgets::pitch_graph(
                ui,
                &p,
                &s.points,
                &s.cracks,
                &s.target,
                self.settings.passaggio_hz,
                170.0,
                View { start: 0.0, end: s.duration.max(1.0) },
                None,
                has_audio.then_some(play_pos),
            );
            if has_audio {
                if let Some(t) = out.clicked_t {
                    seek = Some(t);
                }
                ui.horizontal(|ui| {
                    let label = if playing { "❚❚  Pause" } else { "▶  Play" };
                    if ghost_button(ui, &p, label).clicked() {
                        toggle = true;
                    }
                    ui.label(RichText::new(format!("{} / {}", mmss(play_pos), mmss(s.duration))).family(bold()).size(14.0));
                    muted(ui, &p, "click the graph to jump");
                });
            }

            let st = &s.stats;
            if st.enough {
                ui.horizontal_wrapped(|ui| {
                    let mut stat = |big: String, small: String| {
                        egui::Frame::none()
                            .fill(p.card2)
                            .rounding(egui::Rounding::same(12.0))
                            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                            .show(ui, |ui| {
                                ui.set_min_width(150.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(big).family(bold()).size(24.0));
                                    ui.label(RichText::new(small).size(12.5).color(p.muted));
                                });
                            });
                    };
                    stat(format!("{} Hz", st.median_hz.round()), format!("median pitch · {}", note_name(st.median_hz)));
                    stat(
                        format!("{}-{}", note_name(st.low_hz), note_name(st.high_hz)),
                        format!("typical range · {:.1} semitones", st.range_st),
                    );
                    stat(format!("{}%", (st.in_target * 100.0).round()), "time in goal range".into());
                    stat(st.resonance.map(resonance_word).unwrap_or("--").into(), "resonance".into());
                    if let Some((c, m, h)) = st.registers {
                        let main = if c >= m && c >= h { "chest" } else if h >= m { "head" } else { "mix" };
                        stat(main.into(), format!("mostly · {:.0}/{:.0}/{:.0}% c/m/h", c * 100.0, m * 100.0, h * 100.0));
                    }
                    stat(format!("{}", st.cracks), "voice cracks".into());
                    stat(mmss(st.voiced_s), "voiced time".into());
                });
            }
            for tip in &s.tips {
                egui::Frame::none()
                    .fill(p.card2)
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(tip);
                    });
            }
            if ai_ready && st.enough {
                egui::Frame::none()
                    .stroke(egui::Stroke::new(1.0_f32, p.faint))
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(14.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            title(ui, "AI coach");
                            muted(ui, &p, "runs on this computer · reads numbers only");
                        });
                        if !s.ai_text.is_empty() {
                            ui.label(&s.ai_text);
                        }
                        if let Some(m) = &self.ai_message {
                            muted(ui, &p, m.clone());
                        }
                        let label = if thinking { "Thinking…" } else if s.ai_text.is_empty() { "Get AI feedback" } else { "Ask again" };
                        if ui.add_enabled(!thinking, egui::Button::new(label)).clicked() {
                            ask_ai = Some(session::coach_prompt(st, &s.target));
                        }
                    });
            }
        });

        if save {
            if let Some((rec, sr)) = self.summary.as_ref().and_then(|s| s.recording.clone()) {
                let name = format!("onicvoco-{}.wav", chrono::Local::now().format("%Y-%m-%d-%H%M"));
                if let Some(path) = rfd::FileDialog::new().set_file_name(name).add_filter("WAV audio", &["wav"]).save_file() {
                    if let Err(e) = audio::save_wav(&path, &rec, sr) {
                        self.error = Some(format!("Couldn't save the recording: {e}"));
                    }
                }
            }
        }
        if let Some(t) = seek {
            self.seek(t);
        }
        if toggle {
            self.toggle_play();
        }
        if close {
            self.summary = None;
            self.player = None;
        }
        if let Some(prompt) = ask_ai {
            if let Some(s) = self.summary.as_mut() {
                s.ai_text.clear();
            }
            self.ai_message = None;
            self.ai_state = AiState::Thinking;
            self.ai.generate(session::COACH_SYSTEM_PROMPT, &prompt);
        }
    }

    // ---------- Read ----------

    fn read(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let target = self.target();
        self.error_banner(ui);
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                self.start_stop_buttons(ui);
                ui.add_space(10.0);
                let (status, good) = self.status(&target);
                let (hz, note) = match self.live.hz {
                    Some(h) => (format!("{} Hz", h.round()), hz_to_note(h).name),
                    None => ("-- Hz".into(), "--".into()),
                };
                ui.label(RichText::new(hz).family(bold()).size(28.0));
                ui.label(RichText::new(note).family(bold()).size(28.0).color(p.muted));
                chip(ui, &p, status, good);
                if let (Some(r), true) = (self.live.register, self.live.hz.is_some()) {
                    chip(ui, &p, r.label(), false);
                }
                if let (Some(r), true) = (self.live.res, self.live.hz.is_some()) {
                    chip(ui, &p, resonance_word(r), false);
                }
            });
            widgets::peak_meter(ui, &p, self.live.peak_db, self.peak_hold.0, self.live.db, self.audio.is_some());
        });
        self.graph_card(ui, &target, 130.0, false);

        let idx = self.settings.passage.min(PASSAGES.len() - 1);
        let passage = &PASSAGES[idx];
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(passage.title).family(bold()).size(22.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, &p, "Next →").clicked() {
                        self.settings.passage = (idx + 1) % PASSAGES.len();
                        self.save();
                    }
                    muted(ui, &p, format!("{} / {}", idx + 1, PASSAGES.len()));
                    if ghost_button(ui, &p, "← Previous").clicked() {
                        self.settings.passage = (idx + PASSAGES.len() - 1) % PASSAGES.len();
                        self.save();
                    }
                });
            });
            muted(ui, &p, format!("Focus: {}", passage.focus));
            ui.add_space(8.0);
            ui.label(RichText::new(passage.text).size(23.0).line_height(Some(36.0)));
        });

        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Quick phrases");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, &p, "Next phrase").clicked() {
                        self.phrase = (self.phrase + 1) % QUICK_PHRASES.len();
                    }
                });
            });
            ui.label(RichText::new(QUICK_PHRASES[self.phrase]).family(bold()).size(28.0));
            muted(ui, &p, "Say it three times: once normally, once with more melody, once softer and lighter.");
        });

        self.summary_card(ui);
    }

    // ---------- Learn ----------

    fn learn(&mut self, ui: &mut Ui) {
        let p = self.pal;
        card(ui, &p, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, g) in GUIDES.iter().enumerate() {
                    let on = i == self.guide;
                    let b = egui::Button::new(RichText::new(g.title).family(bold()).size(14.5).color(if on { p.on_text } else { p.text }))
                        .fill(if on { p.text } else { p.card2 })
                        .rounding(egui::Rounding::same(99.0));
                    if ui.add(b).clicked() {
                        self.guide = i;
                    }
                }
            });
        });
        let g = &GUIDES[self.guide];
        card(ui, &p, |ui| {
            ui.label(RichText::new(g.title).family(bold()).size(30.0));
            ui.label(RichText::new(g.summary).size(17.0).color(p.muted));
            ui.add_space(8.0);
            for para in g.body {
                ui.label(RichText::new(*para).size(16.0).line_height(Some(25.0)));
                ui.add_space(4.0);
            }
        });
        card(ui, &p, |ui| {
            title(ui, "Exercises");
            for (name, how) in g.exercises {
                egui::Frame::none()
                    .fill(p.card2)
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(14.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(*name).family(bold()).size(16.5));
                        ui.label(RichText::new(*how).size(15.0).line_height(Some(23.0)));
                    });
            }
        });
    }

    // ---------- Progress ----------

    // ---------- Range ----------

    fn range_tab(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let target = self.target();
        self.error_banner(ui);
        let mapping = self.map_run.is_some();

        card(ui, &p, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    title(ui, "Vocal range");
                    ui.set_max_width(520.0);
                    muted(
                        ui,
                        &p,
                        "Your range fills in on its own whenever the mic is live. For a quick full map, press Map my range and glide from your lowest note to your highest.",
                    );
                    ui.add_space(4.0);
                    self.mic_combo(ui);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !mapping {
                        if primary_button(ui, &p, "Map my range", false).clicked() {
                            self.start_map();
                        }
                    } else if primary_button(ui, &p, "■  Done", true).clicked() {
                        self.finish_map();
                    }
                });
            });
        });

        if let Some(m) = &self.map_run {
            let (low, high) = (m.low, m.high);
            card(ui, &p, |ui| {
                title(ui, "Mapping your range");
                ui.label("On a quiet \"oo\" or \"ng\", glide slowly from your lowest comfortable note up to your highest, then press Done. Stay gentle: never push past what feels easy.");
                ui.horizontal(|ui| {
                    let cur = self.live.hz.map(|h| hz_to_note(h).name).unwrap_or_else(|| "--".into());
                    ui.label(RichText::new(cur).family(bold()).size(34.0));
                    ui.add_space(16.0);
                    match low.zip(high) {
                        Some((a, b)) => muted(ui, &p, format!("so far {} to {}", midi_name(a), midi_name(b))),
                        None => muted(ui, &p, "waiting for your voice…"),
                    }
                });
            });
        } else if let Some((a, b)) = self.map_result {
            card(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    title(ui, "Last glide");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ghost_button(ui, &p, "Close").clicked() {
                            self.map_result = None;
                        }
                    });
                });
                ui.label(RichText::new(format!("{} to {}", midi_name(a), midi_name(b))).family(bold()).size(30.0));
                muted(ui, &p, span_text(b - a));
            });
        }

        let range = self.profile.range();
        let comfort = self.profile.comfort_zone();
        let session = self.map_run.as_ref().and_then(|m| m.low.zip(m.high));
        let live_midi = self.live.hz.map(hz_to_midi);
        let grow = (self.tab_changed.elapsed().as_secs_f32() / 1.0).min(1.0);
        if grow < 1.0 {
            ui.ctx().request_repaint();
        }
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Range map");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    muted(ui, &p, "bar height = time on each note · ◯ = crack · shaded = goal · line under = comfort zone");
                });
            });
            widgets::range_map(
                ui,
                &p,
                &RangeOpts {
                    notes: &self.profile.notes,
                    target: &target,
                    passaggio: self.settings.passaggio_hz,
                    live_midi,
                    session,
                    range,
                    comfort,
                    grow,
                    height: 230.0,
                },
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("■ chest").color(p.text).size(12.5));
                ui.label(RichText::new("■ mix").color(widgets::fade(p.text, 0.55)).size(12.5));
                ui.label(RichText::new("■ head / falsetto").color(widgets::fade(p.text, 0.28)).size(12.5));
            });
        });

        card(ui, &p, |ui| {
            title(ui, "Your range");
            match range {
                Some((lo, hi)) => {
                    ui.columns(4, |c| {
                        stat_tile(&mut c[0], &p, "Lowest", &midi_name(lo));
                        stat_tile(&mut c[1], &p, "Highest", &midi_name(hi));
                        stat_tile(&mut c[2], &p, "Span", &span_text(hi - lo));
                        let cz = comfort.map(|(a, b)| format!("{} to {}", midi_name(a), midi_name(b))).unwrap_or_else(|| "--".into());
                        stat_tile(&mut c[3], &p, "Comfort zone", &cz);
                    });
                    // how much of the goal range has been reached
                    let g0 = hz_to_midi(target.min_hz).round() as i32;
                    let g1 = hz_to_midi(target.max_hz).round() as i32;
                    let total = (g1 - g0 + 1).max(1);
                    let covered = (g0..=g1)
                        .filter(|m| self.profile.notes.get((m - NOTE_MIN) as usize).is_some_and(|n| n.mapped()))
                        .count() as i32;
                    ui.add_space(6.0);
                    ui.label(format!(
                        "You've sounded {covered} of the {total} notes in your {} goal range ({} to {}).",
                        target.label().to_lowercase(),
                        midi_name(g0),
                        midi_name(g1)
                    ));
                    let mut hot: Vec<(usize, f32)> =
                        self.profile.notes.iter().enumerate().filter(|(_, n)| n.cracks > 0.0).map(|(i, n)| (i, n.cracks)).collect();
                    hot.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                    if !hot.is_empty() {
                        let list: Vec<String> =
                            hot.iter().take(3).map(|(i, c)| format!("{} ({}x)", midi_name(NOTE_MIN + *i as i32), *c as u32)).collect();
                        ui.label(format!("Your voice cracks most around {}. That's your practice zone: quiet, light sirens across it.", list.join(", ")));
                    }
                }
                None => {
                    muted(ui, &p, "Nothing mapped yet. Sing or speak with the mic live, or press Map my range and glide. A note counts once you've sounded it for a moment.");
                }
            }
            ui.add_space(8.0);
            let label = if self.confirm_range { "Click again to clear the range map" } else { "Clear range map" };
            if ghost_button(ui, &p, label).clicked() {
                if self.confirm_range {
                    self.profile.reset_range();
                    self.map_result = None;
                    self.save_profile();
                }
                self.confirm_range = !self.confirm_range;
            }
        });
    }

    // ---------- Progress ----------

    fn web_card(&mut self, ui: &mut Ui) {
        let p = self.pal;
        let day = today();
        let axes = self.profile.axes(day);
        let hl = self.profile.highlights(day);
        let grow = (self.tab_changed.elapsed().as_secs_f32() / 1.1).min(1.0);
        if grow < 1.0 {
            ui.ctx().request_repaint();
        }
        let minutes = (self.profile.total_secs() / 60.0).round() as u32;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Your voice web");
                chip(ui, &p, "builds as you practise", false);
            });
            ui.horizontal_top(|ui| {
                widgets::skill_web(ui, &p, &axes, 340.0, grow);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_width(240.0);
                    match hl {
                        Some((best, worst)) => {
                            muted(ui, &p, "Strongest right now");
                            ui.label(RichText::new(axes[best].name).family(bold()).size(22.0));
                            ui.add_space(6.0);
                            muted(ui, &p, "Most room to grow");
                            ui.label(RichText::new(axes[worst].name).family(bold()).size(22.0));
                            ui.label(profile::focus_tip(worst));
                        }
                        None => {
                            ui.label("Keep the mic live while you practise and the web grows on its own. After a few minutes of voice it can start to say what's strong and what has room to grow.");
                        }
                    }
                    ui.add_space(8.0);
                    muted(ui, &p, format!("{minutes} min of voice so far · filled dot = enough data, hollow = still learning · hover an axis for what it means"));
                    ui.add_space(6.0);
                    let label = if self.confirm_web { "Click again to start over" } else { "Start the web over" };
                    if ghost_button(ui, &p, label).clicked() {
                        if self.confirm_web {
                            self.profile.reset_web();
                            self.save_profile();
                        }
                        self.confirm_web = !self.confirm_web;
                    }
                });
            });
        });
    }

    fn progress(&mut self, ui: &mut Ui) {
        let p = self.pal;
        self.web_card(ui);
        if self.history.is_empty() {
            card(ui, &p, |ui| {
                title(ui, "Progress");
                muted(ui, &p, "Finish a recording and your progress will show up here. Everything stays on this computer.");
            });
            return;
        }
        card(ui, &p, |ui| {
            title(ui, "Median pitch over your last sessions");
            let pts: Vec<f32> = self.history.iter().take(30).rev().map(|h| h.median_hz).collect();
            trend(ui, &p, &pts, 130.0);
        });
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Sessions");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if self.confirm_clear { "Click again to clear everything" } else { "Clear history" };
                    if ghost_button(ui, &p, label).clicked() {
                        if self.confirm_clear {
                            self.history.clear();
                            storage::save_history(&self.history);
                        }
                        self.confirm_clear = !self.confirm_clear;
                    }
                });
            });
            egui::Grid::new("hist").num_columns(6).spacing(vec2(24.0, 10.0)).striped(true).show(ui, |ui| {
                for h in ["Date", "Goal", "Median", "In range", "Resonance", "Cracks"] {
                    ui.label(RichText::new(h).color(p.muted).size(13.0));
                }
                ui.end_row();
                for e in self.history.iter().take(60) {
                    use chrono::TimeZone;
                    let when = chrono::Local
                        .timestamp_opt(e.at, 0)
                        .single()
                        .map(|d| d.format("%d %b  %H:%M").to_string())
                        .unwrap_or_default();
                    ui.label(when);
                    ui.label(&e.goal);
                    ui.label(RichText::new(format!("{} Hz  {}", e.median_hz, e.note)).family(bold()));
                    ui.label(format!("{}%", (e.in_target * 100.0).round()));
                    ui.label(e.resonance.map(resonance_word).unwrap_or("--"));
                    ui.label(e.cracks.to_string());
                    ui.end_row();
                }
            });
        });
    }

    // ---------- Settings ----------

    fn settings_tab(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let p = self.pal;
        card(ui, &p, |ui| {
            title(ui, "Appearance");
            let opts = ["System", "Light", "Dark"];
            let keys = ["system", "light", "dark"];
            let idx = keys.iter().position(|k| *k == self.settings.theme).unwrap_or(0);
            if let Some(i) = segmented(ui, &p, "theme", &opts, idx) {
                self.settings.theme = keys[i].into();
                self.save();
                self.refresh_theme(ctx, true);
            }
        });
        card(ui, &p, |ui| {
            title(ui, "Microphone");
            if ui
                .checkbox(&mut self.settings.live_mic, "Keep the microphone live on Practice and Read (pitch and peak meter without recording)")
                .changed()
            {
                self.save();
            }
            ui.horizontal(|ui| {
                ui.label("Noise gate");
                if ui.add(egui::Slider::new(&mut self.settings.gate_db, -70.0..=-25.0).suffix(" dB")).changed() {
                    self.analyzer.gate_db = self.settings.gate_db;
                    self.save();
                }
            });
            muted(ui, &p, "Raise it if background noise shows up as pitch. Lower it if quiet speech is ignored.");
            if ui.checkbox(&mut self.settings.record, "Keep the audio of each recording so I can play it back or save it").changed() {
                self.save();
            }
            muted(ui, &p, "Recordings stay in memory until you save them. Nothing is uploaded.");
        });
        card(ui, &p, |ui| {
            title(ui, "Passaggio");
            match self.settings.passaggio_hz {
                Some(h) => {
                    ui.label(format!("Saved: around {} ({} Hz)", note_name(h), h.round()));
                    if ghost_button(ui, &p, "Forget it").clicked() {
                        self.settings.passaggio_hz = None;
                        self.save();
                    }
                }
                None => muted(ui, &p, "Not measured yet. Use \"Find my passaggio\" on the Practice tab."),
            }
        });
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "AI coach");
                chip(ui, &p, "optional", false);
            });
            ui.label(format!(
                "Off by default and not part of the app itself. If you want it, OnicVoco downloads it separately (a small helper program plus the Qwen2.5 0.5B language model, about {} MB in total) once. After that it runs entirely on this computer's CPU. It never hears your voice: it only reads the numbers from your session summary, and nothing is sent anywhere.",
                ai::DOWNLOAD_SIZE_MB
            ));
            let downloading = match &self.ai_state {
                AiState::Downloading(f, s) => Some((*f, s.clone())),
                _ => None,
            };
            match downloading {
                Some((f, s)) => {
                    widgets::bar(ui, &p, f, 8.0);
                    muted(ui, &p, format!("Downloading… {s}"));
                }
                None => {
                    if self.ai_installed {
                        if ui.checkbox(&mut self.settings.ai_enabled, "Show the AI coach in session summaries").changed() {
                            self.save();
                        }
                        if ghost_button(ui, &p, "Remove AI coach from this computer").clicked() {
                            match ai::remove() {
                                Ok(()) => {
                                    self.ai_installed = false;
                                    self.settings.ai_enabled = false;
                                    self.save();
                                    self.ai_message = Some("AI coach removed.".into());
                                }
                                Err(e) => self.ai_message = Some(format!("Couldn't remove it: {e}")),
                            }
                        }
                    } else if primary_button(ui, &p, "Download AI coach", false).clicked() {
                        self.ai_state = AiState::Downloading(0.0, "starting".into());
                        self.ai_message = None;
                        self.ai.download();
                    }
                }
            }
            if let Some(m) = &self.ai_message {
                muted(ui, &p, m.clone());
            }
        });
        card(ui, &p, |ui| {
            title(ui, "Your data");
            muted(ui, &p, format!("Settings and progress are saved in {}", storage::data_dir().display()));
        });
    }
}

/// Simple line chart for the progress tab.
fn trend(ui: &mut Ui, p: &Pal, pts: &[f32], height: f32) {
    use eframe::egui::{pos2, Align2, FontId, Sense, Stroke};
    if pts.len() < 2 {
        muted(ui, p, "Do a couple more sessions to see a trend.");
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    let grow = ui.ctx().animate_value_with_time(egui::Id::new("trend-grow"), 1.0, 0.6);
    let lo = pts.iter().cloned().fold(f32::MAX, f32::min) - 10.0;
    let hi = pts.iter().cloned().fold(f32::MIN, f32::max) + 10.0;
    let left = rect.left() + 46.0;
    let x = |i: usize| left + (rect.right() - left - 8.0) * i as f32 / (pts.len() - 1) as f32;
    let y = |v: f32| rect.bottom() - 8.0 - (v - lo) / (hi - lo) * (rect.height() - 16.0);
    for v in [lo + 10.0, hi - 10.0] {
        painter.line_segment([pos2(left, y(v)), pos2(rect.right(), y(v))], Stroke::new(0.5_f32, p.stroke));
        painter.text(pos2(rect.left(), y(v)), Align2::LEFT_CENTER, format!("{} Hz", v.round()), FontId::proportional(11.0), p.muted);
    }
    let n = ((pts.len() as f32) * grow).ceil().max(2.0) as usize;
    let line: Vec<_> = pts.iter().take(n).enumerate().map(|(i, &v)| pos2(x(i), y(v))).collect();
    painter.add(egui::Shape::line(line.clone(), Stroke::new(2.0_f32, p.text)));
    for pt in line {
        painter.circle_filled(pt, 3.0, p.text);
    }
}

fn span_text(semitones: i32) -> String {
    let (o, st) = (semitones / 12, semitones % 12);
    match (o, st) {
        (0, st) => format!("{st} semitones"),
        (o, 0) => format!("{o} octave{}", if o == 1 { "" } else { "s" }),
        (o, st) => format!("{o} oct + {st} st"),
    }
}

fn stat_tile(ui: &mut Ui, p: &Pal, label: &str, value: &str) {
    ui.vertical(|ui| {
        muted(ui, p, label);
        ui.label(RichText::new(value).family(bold()).size(22.0));
    });
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_profile();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_theme(ctx, false);
        self.manage_mic();
        self.tick_audio(ctx);
        self.poll_player(ctx);
        self.poll_ai();
        if self.tab != Tab::Range && self.map_run.is_some() {
            self.finish_map();
        }
        if self.profile.dirty && self.profile_saved.elapsed() > Duration::from_secs(12) {
            self.save_profile();
        }
        if !matches!(self.ai_state, AiState::Idle) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        // Space bar starts/stops a recording when not typing.
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Space)) && matches!(self.tab, Tab::Practice | Tab::Read) {
            if self.running.is_some() {
                self.stop();
            } else {
                self.start(Mode::Practice);
            }
        }

        let p = self.pal;
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin::symmetric(24.0, 14.0)))
            .show_separator_line(false)
            .show(ctx, |ui| self.top_bar(ui));

        let appear = (self.tab_changed.elapsed().as_secs_f32() / 0.3).min(1.0);
        if appear < 1.0 {
            ctx.request_repaint();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin { left: 24.0, right: 24.0, top: 4.0, bottom: 0.0 }))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    widgets::appear(ui, appear);
                    match self.tab {
                        Tab::Practice => self.practice(ui),
                        Tab::Read => self.read(ui),
                        Tab::Learn => self.learn(ui),
                        Tab::Range => self.range_tab(ui),
                        Tab::Progress => self.progress(ui),
                        Tab::Settings => self.settings_tab(ui, ctx),
                    }
                    ui.add_space(24.0);
                });
            });
    }
}
