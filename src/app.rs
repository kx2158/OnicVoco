use crate::ai::{self, AiCoach, Msg};
use crate::analyzer::{Analyzer, LiveFrame, FRAME};
use crate::audio::{self, AudioInput};
use crate::content::{GUIDES, PASSAGES, QUICK_PHRASES};
use crate::dsp::cracks::Crack;
use crate::dsp::notes::{format_cents, hz_to_note, note_name};
use crate::session::{self, find_passaggio, resonance_word, Frame, Goal, Passaggio, Stats, Target};
use crate::storage::{self, HistoryEntry, Settings};
use crate::theme::{self, bold, Pal};
use crate::widgets::{self, card, chip, ghost_button, muted, primary_button, segmented, title};
use eframe::egui::{self, vec2, Align, Key, Layout, RichText, Ui};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Practice,
    Read,
    Learn,
    Progress,
    Settings,
}

const TABS: [(Tab, &str); 5] = [
    (Tab::Practice, "Practice"),
    (Tab::Read, "Read"),
    (Tab::Learn, "Learn"),
    (Tab::Progress, "Progress"),
    (Tab::Settings, "Settings"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Practice,
    Passaggio,
}

struct Running {
    mode: Mode,
    started: Instant,
    last_analysis: Instant,
    frames: Vec<Frame>,
    cracks: Vec<Crack>,
}

struct Summary {
    stats: Stats,
    tips: Vec<String>,
    target: Target,
    recording: Option<(Vec<f32>, u32)>,
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
    devices: Vec<String>,
    audio: Option<AudioInput>,
    analyzer: Analyzer,
    buf: Vec<f32>,
    running: Option<Running>,
    live: LiveFrame,
    graph: VecDeque<(f32, Option<f32>)>,
    graph_cracks: Vec<Crack>,
    summary: Option<Summary>,
    passaggio_result: Option<Passaggio>,
    toast: Option<(String, Instant)>,
    error: Option<String>,
    ai: AiCoach,
    ai_state: AiState,
    ai_installed: bool,
    ai_message: Option<String>,
    guide: usize,
    phrase: usize,
    confirm_clear: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let settings = storage::load_settings();
        let pal = theme::resolve(&settings.theme);
        theme::install(&cc.egui_ctx, &pal);
        App {
            pal,
            applied_dark: Some(pal.dark),
            last_theme_check: Instant::now(),
            analyzer: Analyzer::new(settings.gate_db),
            history: storage::load_history(),
            tab: Tab::Practice,
            devices: audio::list_devices(),
            audio: None,
            buf: Vec::with_capacity(FRAME),
            running: None,
            live: LiveFrame::default(),
            graph: VecDeque::new(),
            graph_cracks: Vec::new(),
            summary: None,
            passaggio_result: None,
            toast: None,
            error: None,
            ai: AiCoach::spawn(cc.egui_ctx.clone()),
            ai_state: AiState::Idle,
            ai_installed: ai::installed(),
            ai_message: None,
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

    // ---------- sessions ----------

    fn start(&mut self, mode: Mode) {
        self.error = None;
        let record = self.settings.record && mode == Mode::Practice;
        match AudioInput::start(self.settings.mic.as_deref(), record) {
            Ok(a) => {
                self.audio = Some(a);
                self.analyzer = Analyzer::new(self.settings.gate_db);
                self.graph.clear();
                self.graph_cracks.clear();
                self.live = LiveFrame::default();
                if mode == Mode::Passaggio {
                    self.passaggio_result = None;
                }
                let now = Instant::now();
                self.running = Some(Running { mode, started: now, last_analysis: now, frames: Vec::new(), cracks: Vec::new() });
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn stop(&mut self) {
        let Some(run) = self.running.take() else { return };
        let audio = self.audio.take();
        let recording = audio.as_ref().and_then(|a| a.take_recording().map(|r| (r, a.sample_rate as u32)));
        drop(audio);
        self.live = LiveFrame::default();

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
                self.summary = Some(Summary {
                    stats,
                    tips,
                    target,
                    recording: recording.filter(|(r, _)| !r.is_empty()),
                    ai_text: String::new(),
                });
            }
        }
    }

    fn tick_audio(&mut self, ctx: &egui::Context) {
        let Some(run) = self.running.as_mut() else { return };
        let Some(audio) = self.audio.as_ref() else { return };
        ctx.request_repaint_after(Duration::from_millis(16));
        if let Some(e) = audio.error() {
            self.error = Some(format!("Microphone stopped: {e}"));
        }
        if run.last_analysis.elapsed() < Duration::from_millis(33) {
            return;
        }
        run.last_analysis = Instant::now();
        if !audio.latest(FRAME, &mut self.buf) {
            return;
        }
        let t = run.started.elapsed().as_secs_f32();
        let f = self.analyzer.analyze(&self.buf, audio.sample_rate, t);
        run.frames.push(Frame { t, hz: f.hz, res: f.res, h1h2: f.h1h2, register: f.register });
        if let Some(c) = f.crack {
            run.cracks.push(c);
            self.graph_cracks.push(c);
            if run.mode == Mode::Practice {
                self.toast = Some((
                    format!(
                        "Crack at {} - get a little quieter and lighter as you approach this note.",
                        note_name(c.from_hz.min(c.to_hz))
                    ),
                    Instant::now(),
                ));
            }
        }
        self.graph.push_back((t, f.hz));
        while self.graph.front().is_some_and(|(pt, _)| *pt < t - 13.0) {
            self.graph.pop_front();
        }
        self.graph_cracks.retain(|c| c.t > t - 13.0);
        self.live = f;
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

    // ---------- top bar ----------

    fn top_bar(&mut self, ui: &mut Ui) {
        let p = self.pal;
        ui.horizontal(|ui| {
            ui.label(RichText::new("OnicVoco").family(bold()).size(22.0));
            ui.add_space(18.0);
            let idx = TABS.iter().position(|(t, _)| *t == self.tab).unwrap_or(0);
            let names: Vec<&str> = TABS.iter().map(|(_, n)| *n).collect();
            if let Some(i) = segmented(ui, &p, &names, idx) {
                self.tab = TABS[i].0;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let icon = if p.dark { "☀" } else { "☾" };
                if ghost_button(ui, &p, icon).on_hover_text("Switch theme").clicked() {
                    self.settings.theme = if p.dark { "light".into() } else { "dark".into() };
                    self.save();
                    self.pal = theme::resolve(&self.settings.theme);
                }
                if self.running.is_some() {
                    chip(ui, &p, "● Listening", true);
                }
            });
        });
    }

    // ---------- shared pieces ----------

    fn start_stop_buttons(&mut self, ui: &mut Ui) {
        let p = self.pal;
        match self.running.as_ref().map(|r| r.mode) {
            None => {
                if primary_button(ui, &p, "Start practice").clicked() {
                    self.start(Mode::Practice);
                }
            }
            Some(_) => {
                if primary_button(ui, &p, "■  Finish").clicked() {
                    self.stop();
                }
            }
        }
    }

    fn status(&self, target: &Target) -> (&'static str, bool) {
        if self.running.is_none() {
            return ("Ready", false);
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
            ui.add_space(4.0);
        }
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
                    let current = self.settings.mic.clone().unwrap_or_else(|| "Default microphone".into());
                    let enabled = self.running.is_none();
                    ui.add_enabled_ui(enabled, |ui| {
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
                    if enabled && ui.small_button("Refresh list").clicked() {
                        self.devices = audio::list_devices();
                    }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    muted(ui, &p, "Goal");
                    let goals: Vec<&str> = Goal::ALL.iter().map(|g| g.label()).collect();
                    let idx = Goal::ALL.iter().position(|g| *g == target.goal).unwrap_or(0);
                    if let Some(i) = segmented(ui, &p, &goals, idx) {
                        self.settings.goal = Goal::ALL[i].key().into();
                        self.save();
                    }
                    if target.goal == Goal::Custom {
                        ui.horizontal(|ui| {
                            let a = ui.add(egui::DragValue::new(&mut self.settings.custom_min).clamp_range(50.0..=700.0).suffix(" Hz"));
                            ui.label("to");
                            let b = ui.add(egui::DragValue::new(&mut self.settings.custom_max).clamp_range(50.0..=700.0).suffix(" Hz"));
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
            self.register_card(&mut cols[1]);
            self.resonance_card(&mut cols[1], &target);
        });

        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Pitch over time");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    muted(ui, &p, "last 12 s · thick line = in range · ◉ = crack");
                });
            });
            widgets::pitch_graph(ui, &p, &self.graph, &self.graph_cracks, &target, self.settings.passaggio_hz, 230.0);
        });

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
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Pitch");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| chip(ui, &p, status, good));
            });
            ui.horizontal(|ui| {
                let (hz, note, cents) = match self.live.hz {
                    Some(h) => {
                        let n = hz_to_note(h);
                        (format!("{}", h.round()), n.name, format_cents(n.cents))
                    }
                    None => ("--".into(), "--".into(), String::new()),
                };
                let c = if self.live.hz.is_some() || self.running.is_none() { p.text } else { p.faint };
                ui.label(RichText::new(hz).family(bold()).size(68.0).color(c));
                ui.label(RichText::new("Hz").size(18.0).color(p.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(cents).size(16.0).color(p.muted));
                    ui.label(RichText::new(note).family(bold()).size(36.0).color(c));
                });
            });
            let level = ((self.live.db + 70.0) / 60.0).clamp(0.0, 1.0);
            widgets::bar(ui, &p, if self.running.is_some() { level } else { 0.0 }, 6.0);
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
                if at.elapsed() < Duration::from_secs(4) {
                    egui::Frame::none()
                        .fill(p.card2)
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| ui.label(RichText::new(msg).size(13.5)));
                }
            }
        });
    }

    fn register_card(&mut self, ui: &mut Ui) {
        let p = self.pal;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Register");
                chip(ui, &p, "estimate", false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(h) = self.live.h1h2 {
                        muted(ui, &p, format!("H1-H2 {h:.1} dB"));
                    }
                });
            });
            widgets::register_meter(ui, &p, if self.live.hz.is_some() { self.live.register } else { None });
            muted(ui, &p, "Based on how strong your first harmonics are. Chest is buzzy, head/falsetto is light and airy, mix sits between.");
        });
    }

    fn resonance_card(&mut self, ui: &mut Ui, target: &Target) {
        let p = self.pal;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Resonance");
                chip(ui, &p, "estimate", false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(f) = self.live.formants {
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
                    ui.label(RichText::new(format!("around {} ({} Hz)", note_name(hz), hz.round())).family(bold()).size(28.0));
                    ui.label(if via_crack {
                        "Your voice cracked here on the way up. This is the zone to practise: quiet, light sirens across it."
                    } else {
                        "Your voice shifted from chest toward head voice here. It's now marked on the graph and note strip."
                    });
                }
                Passaggio::Smooth { low_hz, high_hz } => {
                    ui.label(RichText::new("No clear break found").family(bold()).size(22.0));
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
        let mut ask_ai = None;
        let mut close = false;
        let Some(s) = self.summary.as_ref() else { return };
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, "Session summary");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ghost_button(ui, &p, "Close").clicked() {
                        close = true;
                    }
                    if let Some((rec, sr)) = &s.recording {
                        if ghost_button(ui, &p, "Save recording…").clicked() {
                            let name = format!("onicvoco-{}.wav", chrono::Local::now().format("%Y-%m-%d-%H%M"));
                            if let Some(path) = rfd::FileDialog::new().set_file_name(name).add_filter("WAV audio", &["wav"]).save_file() {
                                if let Err(e) = audio::save_wav(&path, rec, *sr) {
                                    self.error = Some(format!("Couldn't save the recording: {e}"));
                                }
                            }
                        }
                    }
                });
            });
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
                                    ui.label(RichText::new(big).family(bold()).size(22.0));
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
                    stat(format!("{} s", st.voiced_s.round()), "voiced time".into());
                });
            }
            ui.add_space(6.0);
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
                ui.add_space(6.0);
                egui::Frame::none()
                    .stroke(egui::Stroke::new(1.0, p.faint))
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
        if close {
            self.summary = None;
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
                ui.label(RichText::new(hz).family(bold()).size(26.0));
                ui.label(RichText::new(note).family(bold()).size(26.0).color(p.muted));
                chip(ui, &p, status, good);
                if let (Some(r), true) = (self.live.register, self.live.hz.is_some()) {
                    chip(ui, &p, r.label(), false);
                }
                if let (Some(r), true) = (self.live.res, self.live.hz.is_some()) {
                    chip(ui, &p, resonance_word(r), false);
                }
            });
            widgets::pitch_graph(ui, &p, &self.graph, &self.graph_cracks, &target, self.settings.passaggio_hz, 120.0);
        });

        let idx = self.settings.passage.min(PASSAGES.len() - 1);
        let passage = &PASSAGES[idx];
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(passage.title).family(bold()).size(20.0));
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
            ui.label(RichText::new(QUICK_PHRASES[self.phrase]).family(bold()).size(26.0));
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
                    let b = egui::Button::new(RichText::new(g.title).family(bold()).size(14.0).color(if on { p.on_text } else { p.text }))
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
            ui.label(RichText::new(g.title).family(bold()).size(28.0));
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
                        ui.label(RichText::new(*name).family(bold()).size(16.0));
                        ui.label(RichText::new(*how).size(15.0).line_height(Some(23.0)));
                    });
            }
        });
    }

    // ---------- Progress ----------

    fn progress(&mut self, ui: &mut Ui) {
        let p = self.pal;
        if self.history.is_empty() {
            card(ui, &p, |ui| {
                title(ui, "Progress");
                muted(ui, &p, "Finish a practice session and your progress will show up here. Everything stays on this computer.");
            });
            return;
        }
        card(ui, &p, |ui| {
            title(ui, "Median pitch over your last sessions");
            let pts: Vec<f32> = self.history.iter().take(30).rev().map(|h| h.median_hz).collect();
            trend(ui, &p, &pts, 120.0);
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
            if let Some(i) = segmented(ui, &p, &opts, idx) {
                self.settings.theme = keys[i].into();
                self.save();
                self.refresh_theme(ctx, true);
            }
        });
        card(ui, &p, |ui| {
            title(ui, "Microphone");
            ui.horizontal(|ui| {
                ui.label("Noise gate");
                if ui.add(egui::Slider::new(&mut self.settings.gate_db, -70.0..=-25.0).suffix(" dB")).changed() {
                    self.analyzer.gate_db = self.settings.gate_db;
                    self.save();
                }
            });
            muted(ui, &p, "Raise it if background noise shows up as pitch. Lower it if quiet speech is ignored.");
            if ui.checkbox(&mut self.settings.record, "Keep a recording of each practice session so I can play it back or save it").changed() {
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
                "Off by default. If you want it, OnicVoco downloads a small language model (Qwen2.5 0.5B, about {} MB) from Hugging Face once. After that it runs entirely on this computer's CPU. It never hears your voice: it only reads the numbers from your session summary, and nothing is sent anywhere.",
                ai::MODEL_SIZE_MB
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
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut self.settings.ai_enabled, "Show the AI coach in session summaries").changed() {
                                self.save();
                            }
                        });
                        if ghost_button(ui, &p, "Remove model from this computer").clicked() {
                            match ai::remove() {
                                Ok(()) => {
                                    self.ai_installed = false;
                                    self.settings.ai_enabled = false;
                                    self.save();
                                    self.ai_message = Some("Model removed.".into());
                                }
                                Err(e) => self.ai_message = Some(format!("Couldn't remove it: {e}")),
                            }
                        }
                    } else if primary_button(ui, &p, "Download AI coach").clicked() {
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
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if pts.len() < 2 {
        muted(ui, p, "Do a couple more sessions to see a trend.");
        return;
    }
    let painter = ui.painter_at(rect);
    let lo = pts.iter().cloned().fold(f32::MAX, f32::min) - 10.0;
    let hi = pts.iter().cloned().fold(f32::MIN, f32::max) + 10.0;
    let left = rect.left() + 46.0;
    let x = |i: usize| left + (rect.right() - left - 8.0) * i as f32 / (pts.len() - 1) as f32;
    let y = |v: f32| rect.bottom() - 8.0 - (v - lo) / (hi - lo) * (rect.height() - 16.0);
    for v in [lo + 10.0, hi - 10.0] {
        painter.line_segment([pos2(left, y(v)), pos2(rect.right(), y(v))], Stroke::new(0.5, p.stroke));
        painter.text(pos2(rect.left(), y(v)), Align2::LEFT_CENTER, format!("{} Hz", v.round()), FontId::proportional(11.0), p.muted);
    }
    let line: Vec<_> = pts.iter().enumerate().map(|(i, &v)| pos2(x(i), y(v))).collect();
    painter.add(egui::Shape::line(line.clone(), Stroke::new(2.0, p.text)));
    for pt in line {
        painter.circle_filled(pt, 3.0, p.text);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_theme(ctx, false);
        self.tick_audio(ctx);
        self.poll_ai();
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(5)) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if !matches!(self.ai_state, AiState::Idle) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        // Space bar starts/stops a practice session when not typing.
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

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin { left: 24.0, right: 24.0, top: 4.0, bottom: 0.0 }))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    match self.tab {
                        Tab::Practice => self.practice(ui),
                        Tab::Read => self.read(ui),
                        Tab::Learn => self.learn(ui),
                        Tab::Progress => self.progress(ui),
                        Tab::Settings => self.settings_tab(ui, ctx),
                    }
                    ui.add_space(24.0);
                });
            });
    }
}
