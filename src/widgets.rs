//! Custom-drawn, animated monochrome widgets.

use crate::dsp::cracks::Crack;
use crate::dsp::notes::{hz_to_midi, is_sharp, midi_name, midi_to_hz};
use crate::dsp::register::Register;
use crate::session::Target;
use crate::theme::{bold, Pal};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, RichText, Rounding, Sense, Shape, Stroke, Ui};

/// Fade a colour toward transparent (works for both themes).
pub fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

pub fn card<R>(ui: &mut Ui, p: &Pal, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.stroke))
        .rounding(Rounding::same(16.0))
        .inner_margin(egui::Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).family(bold()).size(16.0));
}

pub fn muted(ui: &mut Ui, p: &Pal, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(p.muted).size(13.0));
}

/// Small pill label. Filled = active state. Fades smoothly between states.
pub fn chip(ui: &mut Ui, p: &Pal, text: &str, filled: bool) {
    let t = ui.ctx().animate_bool_with_time(ui.id().with(("chip", text)), filled, 0.2);
    let fill = lerp_color(p.card2, p.text, t);
    let fg = lerp_color(p.muted, p.on_text, t);
    egui::Frame::none()
        .fill(fill)
        .rounding(Rounding::same(99.0))
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| ui.label(RichText::new(text).color(fg).size(12.5).family(bold())));
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Primary pill button (solid, inverted). `recording` adds a soft pulsing halo.
pub fn primary_button(ui: &mut Ui, p: &Pal, text: &str, recording: bool) -> egui::Response {
    let r = ui.add(
        egui::Button::new(RichText::new(text).color(p.on_text).family(bold()).size(15.5))
            .fill(p.text)
            .rounding(Rounding::same(99.0))
            .min_size(vec2(170.0, 40.0)),
    );
    if recording {
        let t = ui.input(|i| i.time) as f32;
        let phase = (t * 1.4).fract();
        let grow = 2.0 + phase * 10.0;
        ui.painter().rect_stroke(
            r.rect.expand(grow),
            Rounding::same(99.0),
            Stroke::new(1.5_f32, fade(p.text, 0.5 * (1.0 - phase))),
        );
        ui.ctx().request_repaint();
    }
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

pub fn ghost_button(ui: &mut Ui, p: &Pal, text: &str) -> egui::Response {
    let r = ui.add(
        egui::Button::new(RichText::new(text).size(14.0))
            .fill(p.card2)
            .stroke(Stroke::new(1.0_f32, p.stroke))
            .rounding(Rounding::same(99.0))
            .min_size(vec2(0.0, 34.0)),
    );
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// Segmented control with a sliding selection pill. Returns a newly picked index.
pub fn segmented(ui: &mut Ui, p: &Pal, key: &str, options: &[&str], selected: usize) -> Option<usize> {
    let font = FontId::new(14.0, bold());
    let pad = vec2(14.0, 7.0);
    let galleys: Vec<_> = options
        .iter()
        .map(|o| ui.painter().layout_no_wrap(o.to_string(), font.clone(), p.text))
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad.x * 2.0).collect();
    let h = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max) + pad.y * 2.0;
    let total = widths.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, h + 6.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_filled(rect, Rounding::same(99.0), p.card2);

    // x offsets of each option
    let mut xs = Vec::with_capacity(options.len());
    let mut x = rect.left() + 3.0;
    for w in &widths {
        xs.push(x);
        x += w;
    }
    let id = ui.id().with(("seg", key));
    let ax = ui.ctx().animate_value_with_time(id.with("x"), xs[selected.min(xs.len() - 1)], 0.22);
    let aw = ui.ctx().animate_value_with_time(id.with("w"), widths[selected.min(widths.len() - 1)], 0.22);
    let pill = Rect::from_min_size(pos2(ax, rect.top() + 3.0), vec2(aw, h));
    painter.rect_filled(pill, Rounding::same(99.0), p.text);

    let mut picked = None;
    for (i, g) in galleys.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(xs[i], rect.top() + 3.0), vec2(widths[i], h));
        let resp = ui.interact(r, id.with(i), Sense::click());
        // text colour follows the pill as it slides past
        let cover = ((pill.right().min(r.right()) - pill.left().max(r.left())) / r.width()).clamp(0.0, 1.0);
        let base = if resp.hovered() { p.text } else { p.muted };
        let color = lerp_color(base, p.on_text, cover);
        let pos = r.center() - g.size() / 2.0;
        painter.galley(pos, g, color);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && i != selected {
            picked = Some(i);
        }
    }
    picked
}

/// Peak level meter in dBFS with a peak-hold tick and a clip warning.
pub fn peak_meter(ui: &mut Ui, p: &Pal, peak_db: f32, hold_db: f32, rms_db: f32, active: bool) {
    let lo = -60.0;
    let to_x = |db: f32, r: Rect| r.left() + r.width() * ((db - lo) / -lo).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 10.0));
    painter.rect_filled(bar, Rounding::same(5.0), p.card2);
    let id = ui.id().with("peak");
    let pk = ui.ctx().animate_value_with_time(id, if active { peak_db } else { lo }, 0.06);
    let rm = ui.ctx().animate_value_with_time(id.with("rms"), if active { rms_db } else { lo }, 0.1);
    if pk > lo {
        let mut r = bar;
        r.set_right(to_x(pk, bar));
        painter.rect_filled(r, Rounding::same(5.0), fade(p.text, 0.35));
    }
    if rm > lo {
        let mut r = bar;
        r.set_right(to_x(rm, bar));
        painter.rect_filled(r, Rounding::same(5.0), p.text);
    }
    if active && hold_db > lo {
        let x = to_x(hold_db, bar);
        painter.line_segment([pos2(x, bar.top() - 2.0), pos2(x, bar.bottom() + 2.0)], Stroke::new(2.0_f32, p.text));
    }
    let f = FontId::proportional(10.0);
    for db in [-48.0, -36.0, -24.0, -12.0, -6.0, 0.0] {
        let x = to_x(db, bar);
        painter.line_segment([pos2(x, bar.bottom() + 2.0), pos2(x, bar.bottom() + 5.0)], Stroke::new(1.0_f32, p.faint));
        painter.text(pos2(x, bar.bottom() + 12.0), Align2::CENTER_CENTER, format!("{db:.0}"), f.clone(), p.muted);
    }
    let label = if !active {
        "mic off".to_string()
    } else if hold_db > -1.0 {
        "TOO LOUD".to_string()
    } else {
        format!("PEAK {hold_db:.0} dB")
    };
    painter.text(pos2(bar.left(), bar.bottom() + 12.0), Align2::LEFT_CENTER, label, FontId::new(10.5, bold()), p.text);
}

/// Dark-to-bright resonance scale with the goal zone outlined. The marker glides.
pub fn resonance_meter(ui: &mut Ui, p: &Pal, value: Option<f32>, zone: Option<(f32, f32)>) {
    let h = 18.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h + 26.0), Sense::hover());
    let track = Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(rect.width(), h));
    let painter = ui.painter_at(rect.expand(8.0));
    let steps = 72;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let g = (36.0 + t * 205.0) as u8;
        let x0 = track.left() + track.width() * i as f32 / steps as f32;
        let x1 = track.left() + track.width() * (i + 1) as f32 / steps as f32 + 0.6;
        painter.rect_filled(Rect::from_x_y_ranges(x0..=x1, track.y_range()), 0.0, Color32::from_gray(g));
    }
    painter.rect_stroke(track.expand(2.0), Rounding::same(h / 2.0 + 2.0), Stroke::new(4.0_f32, p.card));
    painter.rect_stroke(track, Rounding::same(h / 2.0), Stroke::new(1.0_f32, p.stroke));
    if let Some((lo, hi)) = zone {
        let z = Rect::from_x_y_ranges(
            track.left() + track.width() * lo..=track.left() + track.width() * hi,
            track.top() - 4.0..=track.bottom() + 4.0,
        );
        let pts = [z.left_top(), z.right_top(), z.right_bottom(), z.left_bottom(), z.left_top()];
        painter.extend(Shape::dashed_line(&pts, Stroke::new(1.5_f32, p.text), 5.0, 4.0));
    }
    let id = ui.id().with("res");
    let shown = ui.ctx().animate_bool_with_time(id.with("on"), value.is_some(), 0.3);
    let v = ui.ctx().animate_value_with_time(id, value.unwrap_or(0.5), 0.35);
    if shown > 0.01 {
        let c = pos2(track.left() + track.width() * v.clamp(0.0, 1.0), track.center().y);
        let r = 7.0 + 4.0 * shown;
        painter.circle_filled(c, r, fade(p.card, shown));
        painter.circle_stroke(c, r, Stroke::new(3.0_f32, fade(p.text, shown)));
    }
    let y = track.bottom() + 14.0;
    let f = FontId::proportional(12.0);
    painter.text(pos2(track.left(), y), Align2::LEFT_CENTER, "darker", f.clone(), p.muted);
    painter.text(pos2(track.center().x, y), Align2::CENTER_CENTER, "balanced", f.clone(), p.muted);
    painter.text(pos2(track.right(), y), Align2::RIGHT_CENTER, "brighter", f, p.muted);
}

/// Three-step register indicator with a sliding highlight.
pub fn register_meter(ui: &mut Ui, p: &Pal, current: Option<Register>) {
    let labels = [("Chest", Register::Chest), ("Mix", Register::Mix), ("Head / falsetto", Register::Head)];
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let gap = 6.0;
    let w = (rect.width() - gap * 2.0) / 3.0;
    let painter = ui.painter_at(rect.expand(2.0));
    let id = ui.id().with("reg");
    let idx = current.and_then(|c| labels.iter().position(|(_, r)| *r == c));
    let shown = ui.ctx().animate_bool_with_time(id.with("on"), idx.is_some(), 0.25);
    let x = ui.ctx().animate_value_with_time(id, idx.unwrap_or(1) as f32, 0.2);
    for (i, (label, _)) in labels.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * (w + gap), 0.0), vec2(w, rect.height()));
        painter.rect_filled(r, Rounding::same(11.0), p.card2);
    }
    let hl = Rect::from_min_size(rect.min + vec2(x * (w + gap), 0.0), vec2(w, rect.height()));
    painter.rect_filled(hl.shrink(2.0 * (1.0 - shown)), Rounding::same(11.0), fade(p.text, shown));
    for (i, (label, _)) in labels.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * (w + gap), 0.0), vec2(w, rect.height()));
        let cover = (1.0 - (x - i as f32).abs()).clamp(0.0, 1.0) * shown;
        painter.text(r.center(), Align2::CENTER_CENTER, *label, FontId::new(14.0, bold()), lerp_color(p.muted, p.on_text, cover));
    }
}

const G_MIN: f32 = 36.0; // C2
const G_MAX: f32 = 74.0; // D5

/// Which slice of time the graph shows.
#[derive(Clone, Copy)]
pub struct View {
    pub start: f32,
    pub end: f32,
}

pub struct GraphOut {
    pub response: egui::Response,
    /// Time under a click, for seeking playback.
    pub clicked_t: Option<f32>,
}

/// Pitch graph with goal band, faint semitone lines, passaggio line, crack
/// markers (with a little pop when they appear) and an optional playhead.
#[allow(clippy::too_many_arguments)]
pub fn pitch_graph(
    ui: &mut Ui,
    p: &Pal,
    points: &[(f32, Option<f32>)],
    cracks: &[Crack],
    target: &Target,
    passaggio: Option<f32>,
    height: f32,
    view: View,
    live_now: Option<f32>,
    playhead: Option<f32>,
) -> GraphOut {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let painter = ui.painter_at(rect);
    let left = rect.left() + 32.0;
    let right = rect.right() - 4.0;
    let (top, bottom) = (rect.top() + 6.0, rect.bottom() - 6.0);
    let y = |hz: f32| {
        let m = hz_to_midi(hz).clamp(G_MIN, G_MAX);
        bottom - (m - G_MIN) / (G_MAX - G_MIN) * (bottom - top)
    };
    let span = (view.end - view.start).max(0.5);
    let x = |t: f32| left + (t - view.start) / span * (right - left);

    // goal band (soft)
    let band = Rect::from_x_y_ranges(left..=right, y(target.max_hz)..=y(target.min_hz));
    painter.rect_filled(band, Rounding::same(8.0), p.card2);

    // Very light lines for every semitone, slightly stronger on C with a label.
    let f = FontId::proportional(10.5);
    for m in G_MIN as i32..=G_MAX as i32 {
        let yy = y(midi_to_hz(m as f32));
        if m.rem_euclid(12) == 0 {
            painter.line_segment([pos2(left, yy), pos2(right, yy)], Stroke::new(1.0_f32, fade(p.stroke, 0.9)));
            painter.text(pos2(rect.left(), yy), Align2::LEFT_CENTER, midi_name(m), f.clone(), p.muted);
        } else {
            painter.line_segment([pos2(left, yy), pos2(right, yy)], Stroke::new(0.5_f32, fade(p.stroke, 0.45)));
        }
    }
    // goal edges labelled with their notes, very quietly
    for hz in [target.min_hz, target.max_hz] {
        painter.text(pos2(right - 2.0, y(hz)), Align2::RIGHT_CENTER, midi_name(hz_to_midi(hz).round() as i32), FontId::proportional(9.5), fade(p.muted, 0.8));
    }

    if let Some(ph) = passaggio {
        let yy = y(ph);
        painter.extend(Shape::dashed_line(&[pos2(left, yy), pos2(right, yy)], Stroke::new(1.2_f32, p.muted), 6.0, 5.0));
        painter.text(pos2(left + 4.0, yy - 8.0), Align2::LEFT_CENTER, "passaggio", f.clone(), p.muted);
    }

    // pitch line: thicker inside the goal range
    let mut run: Vec<Pos2> = Vec::new();
    let mut run_in: Option<bool> = None;
    let flush = |run: &mut Vec<Pos2>, inside: Option<bool>| {
        if run.len() >= 2 {
            let w = if inside == Some(true) { 3.2_f32 } else { 1.8 };
            painter.add(Shape::line(std::mem::take(run), Stroke::new(w, p.text)));
        } else {
            run.clear();
        }
    };
    for &(t, hz) in points {
        if t < view.start - 0.1 || t > view.end + 0.1 {
            continue;
        }
        match hz {
            Some(h) => {
                let inside = target.contains(h);
                if run_in.is_some() && run_in != Some(inside) {
                    let last = *run.last().unwrap();
                    flush(&mut run, run_in);
                    run.push(last);
                }
                run_in = Some(inside);
                run.push(pos2(x(t), y(h)));
            }
            None => {
                flush(&mut run, run_in);
                run_in = None;
            }
        }
    }
    flush(&mut run, run_in);

    // cracks: a vertical connector, a dot, and an expanding ring when fresh
    let now = live_now.unwrap_or(view.end);
    for c in cracks.iter().filter(|c| c.t >= view.start && c.t <= view.end) {
        let cx = x(c.t);
        let (y1, y2) = (y(c.from_hz), y(c.to_hz));
        painter.extend(Shape::dashed_line(&[pos2(cx, y1), pos2(cx, y2)], Stroke::new(1.0_f32, p.muted), 3.0, 3.0));
        painter.circle_filled(pos2(cx, y2), 4.5, p.text);
        painter.circle_stroke(pos2(cx, y2), 8.0, Stroke::new(1.0_f32, p.text));
        let age = now - c.t;
        if live_now.is_some() && (0.0..0.8).contains(&age) {
            let k = age / 0.8;
            painter.circle_stroke(pos2(cx, y2), 8.0 + 22.0 * k, Stroke::new(2.0_f32, fade(p.text, 1.0 - k)));
            ui.ctx().request_repaint();
        }
    }

    if let Some(ph) = playhead {
        let px = x(ph);
        painter.line_segment([pos2(px, top), pos2(px, bottom)], Stroke::new(1.5_f32, p.text));
        painter.circle_filled(pos2(px, top + 3.0), 4.0, p.text);
    }

    if points.iter().all(|(_, h)| h.is_none()) {
        painter.text(rect.center(), Align2::CENTER_CENTER, "say something ~", FontId::proportional(13.0), fade(p.muted, 0.7));
    }

    let clicked_t = response
        .clicked()
        .then(|| response.interact_pointer_pos())
        .flatten()
        .map(|pos| view.start + ((pos.x - left) / (right - left)).clamp(0.0, 1.0) * span);
    GraphOut { response, clicked_t }
}

/// Chromatic strip E2..C5. Goal notes tinted, current note inverted and lifted.
pub fn note_strip(ui: &mut Ui, p: &Pal, current: Option<i32>, target: &Target, passaggio: Option<f32>) {
    let first = 40; // E2
    let last = 72; // C5
    let count = (last - first + 1) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 80.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let gap = 3.0;
    let w = (rect.width() - gap * (count - 1.0)) / count;
    let lo = hz_to_midi(target.min_hz).round() as i32;
    let hi = hz_to_midi(target.max_hz).round() as i32;
    let pm = passaggio.map(|h| hz_to_midi(h).round() as i32);
    for m in first..=last {
        let i = (m - first) as f32;
        let sharp = is_sharp(m);
        let h = if sharp { 52.0 } else { 72.0 };
        let on = ui.ctx().animate_bool_with_time(ui.id().with(("key", m)), current == Some(m), 0.12);
        let lift = 5.0 * on;
        let r = Rect::from_min_size(pos2(rect.left() + i * (w + gap), rect.top() + 6.0 - lift), vec2(w, h));
        let in_goal = m >= lo && m <= hi;
        let base = if in_goal {
            if p.dark { Color32::from_gray(if sharp { 70 } else { 58 }) } else { Color32::from_gray(if sharp { 196 } else { 214 }) }
        } else if sharp {
            p.card2
        } else {
            p.card
        };
        painter.rect_filled(r, Rounding::same(7.0), lerp_color(base, p.text, on));
        painter.rect_stroke(r, Rounding::same(7.0), Stroke::new(1.0_f32, p.stroke));
        if pm == Some(m) {
            painter.circle_filled(pos2(r.center().x, r.top() + 8.0), 3.0, lerp_color(p.text, p.on_text, on));
        }
        if !sharp && w > 14.0 {
            painter.text(
                pos2(r.center().x, r.bottom() - 10.0),
                Align2::CENTER_CENTER,
                midi_name(m),
                FontId::proportional(if w > 24.0 { 10.5 } else { 8.5 }),
                lerp_color(p.muted, p.on_text, on),
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Sleepy,
    Listening,
    Happy,
    Oops,
}

/// Voco, the little round face hiding in the corner. It blinks, its mouth
/// moves with your voice, it smiles when you hold your goal, and winces at cracks.
pub fn mascot(ui: &mut Ui, p: &Pal, mood: Mood, level: f32) -> egui::Response {
    let size = 34.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let t = ui.input(|i| i.time) as f32;
    let id = ui.id().with("voco");
    let wiggle_until = ui.ctx().data(|d| d.get_temp::<f32>(id).unwrap_or(0.0));
    if resp.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(id, t + 0.6));
    }
    let wig = if t < wiggle_until { ((wiggle_until - t) * 30.0).sin() * 3.0 * (wiggle_until - t) / 0.6 } else { 0.0 };
    let bob = (t * 2.0).sin() * if mood == Mood::Sleepy { 0.6 } else { 1.2 };
    let c = rect.center() + vec2(wig, bob);
    let painter = ui.painter_at(rect.expand(10.0));
    let r = size / 2.0 - 1.0;
    painter.circle_filled(c, r, p.text);
    let ink = p.on_text;

    // eyes
    let ex = r * 0.38;
    let ey = c.y - r * 0.18;
    let blink = {
        let cyc = (t + 0.7) % 4.2;
        cyc < 0.12
    };
    for side in [-1.0, 1.0] {
        let e = pos2(c.x + side * ex, ey);
        match mood {
            Mood::Sleepy => {
                painter.line_segment([e - vec2(3.0, 0.0), e + vec2(3.0, 0.0)], Stroke::new(1.6_f32, ink));
            }
            Mood::Happy => {
                // ^ ^
                painter.add(Shape::line(vec![e + vec2(-3.0, 1.5), e + vec2(0.0, -1.8), e + vec2(3.0, 1.5)], Stroke::new(1.6_f32, ink)));
            }
            Mood::Oops => {
                // > <
                let s = side;
                painter.add(Shape::line(
                    vec![e + vec2(-2.5 * s, -2.5), e + vec2(2.0 * s, 0.0), e + vec2(-2.5 * s, 2.5)],
                    Stroke::new(1.6_f32, ink),
                ));
            }
            Mood::Listening => {
                if blink {
                    painter.line_segment([e - vec2(2.5, 0.0), e + vec2(2.5, 0.0)], Stroke::new(1.6_f32, ink));
                } else {
                    painter.circle_filled(e, 2.2, ink);
                }
            }
        }
    }
    // mouth: a little wave that moves with your voice
    let amp = match mood {
        Mood::Sleepy => 0.0,
        _ => 0.6 + 3.2 * level.clamp(0.0, 1.0),
    };
    let my = c.y + r * 0.38;
    let pts: Vec<Pos2> = (0..=16)
        .map(|i| {
            let u = i as f32 / 16.0;
            let wave = if mood == Mood::Happy { -(u * std::f32::consts::PI).sin() * 2.5 } else { (u * 6.28 * 1.5 + t * 9.0).sin() * amp * (u * std::f32::consts::PI).sin() };
            pos2(c.x - r * 0.45 + u * r * 0.9, my + wave)
        })
        .collect();
    painter.add(Shape::line(pts, Stroke::new(1.6_f32, ink)));
    if mood == Mood::Sleepy {
        let z = ((t * 0.8) % 1.0) * 8.0;
        painter.text(pos2(rect.right() + 3.0, rect.top() - z + 4.0), Align2::LEFT_CENTER, "z", FontId::proportional(10.0), fade(p.muted, 1.0 - z / 8.0));
    }
    if mood == Mood::Happy {
        for k in 0..3 {
            let a = t * 1.6 + k as f32 * 2.1;
            let s = pos2(c.x + a.cos() * (r + 7.0), c.y + a.sin() * (r + 7.0));
            painter.text(s, Align2::CENTER_CENTER, "✦", FontId::proportional(8.0 + (a * 2.0).sin().abs() * 3.0), p.text);
        }
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    resp.on_hover_text(match mood {
        Mood::Sleepy => "Voco is napping. Open Practice or Read to wake them up.",
        Mood::Listening => "hi, I'm Voco ~",
        Mood::Happy => "you're right in your range!",
        Mood::Oops => "oops, a crack! lighter and softer near that note",
    })
}

/// Fades content in and slides it up slightly after a tab switch.
/// `t` runs 0..1 over the transition.
pub fn appear(ui: &mut Ui, t: f32) {
    let ease = 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
    ui.add_space(10.0 * (1.0 - ease));
    ui.set_opacity(ease);
}

/// Horizontal progress bar.
pub fn bar(ui: &mut Ui, p: &Pal, frac: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let r = Rounding::same(height / 2.0);
    ui.painter().rect_filled(rect, r, p.card2);
    let shown = ui.ctx().animate_value_with_time(ui.id().with("bar"), frac.clamp(0.0, 1.0), 0.3);
    let mut fill = rect;
    fill.set_width(rect.width() * shown);
    if fill.width() > 0.5 {
        ui.painter().rect_filled(fill, r, p.text);
    }
}
