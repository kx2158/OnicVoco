//! Custom-drawn monochrome widgets.

use crate::dsp::cracks::Crack;
use crate::dsp::notes::{hz_to_midi, is_sharp, midi_name, midi_to_hz};
use crate::dsp::register::Register;
use crate::session::Target;
use crate::theme::{bold, Pal};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Rounding, Sense, Shape, Stroke, Ui};
use std::collections::VecDeque;

pub fn card<R>(ui: &mut Ui, p: &Pal, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.stroke))
        .rounding(Rounding::same(16.0))
        .inner_margin(egui::Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).family(bold()).size(15.0));
}

pub fn muted(ui: &mut Ui, p: &Pal, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(p.muted).size(13.0));
}

/// Small pill label. Filled = active state.
pub fn chip(ui: &mut Ui, p: &Pal, text: &str, filled: bool) {
    let (fill, fg) = if filled { (p.text, p.on_text) } else { (p.card2, p.muted) };
    egui::Frame::none()
        .fill(fill)
        .rounding(Rounding::same(99.0))
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| ui.label(RichText::new(text).color(fg).size(12.5).family(bold())));
}

/// Primary pill button (solid, inverted colours).
pub fn primary_button(ui: &mut Ui, p: &Pal, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(p.on_text).family(bold()).size(15.0))
            .fill(p.text)
            .rounding(Rounding::same(99.0))
            .min_size(vec2(170.0, 40.0)),
    )
}

pub fn ghost_button(ui: &mut Ui, p: &Pal, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(14.0))
            .fill(p.card2)
            .stroke(Stroke::new(1.0, p.stroke))
            .rounding(Rounding::same(99.0))
            .min_size(vec2(0.0, 34.0)),
    )
}

/// Segmented control. Returns the newly selected index, if changed.
pub fn segmented(ui: &mut Ui, p: &Pal, options: &[&str], selected: usize) -> Option<usize> {
    let mut out = None;
    egui::Frame::none()
        .fill(p.card2)
        .rounding(Rounding::same(99.0))
        .inner_margin(egui::Margin::same(3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, o) in options.iter().enumerate() {
                    let on = i == selected;
                    let text = RichText::new(*o)
                        .size(13.5)
                        .family(bold())
                        .color(if on { p.on_text } else { p.muted });
                    let b = egui::Button::new(text)
                        .fill(if on { p.text } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(99.0))
                        .min_size(vec2(0.0, 28.0));
                    if ui.add(b).clicked() && !on {
                        out = Some(i);
                    }
                }
            });
        });
    out
}

/// Horizontal level/progress bar.
pub fn bar(ui: &mut Ui, p: &Pal, frac: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let r = Rounding::same(height / 2.0);
    ui.painter().rect_filled(rect, r, p.card2);
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    if fill.width() > 0.5 {
        ui.painter().rect_filled(fill, r, p.text);
    }
}

/// Dark-to-bright resonance scale with the goal zone outlined.
pub fn resonance_meter(ui: &mut Ui, p: &Pal, value: Option<f32>, zone: Option<(f32, f32)>) {
    let h = 18.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h + 26.0), Sense::hover());
    let track = Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(rect.width(), h));
    let painter = ui.painter_at(rect.expand(6.0));
    // grey gradient, drawn as thin slices
    let steps = 60;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let g = (40.0 + t * 200.0) as u8;
        let x0 = track.left() + track.width() * i as f32 / steps as f32;
        let x1 = track.left() + track.width() * (i + 1) as f32 / steps as f32 + 0.5;
        painter.rect_filled(Rect::from_x_y_ranges(x0..=x1, track.y_range()), 0.0, Color32::from_gray(g));
    }
    painter.rect_stroke(track, Rounding::same(h / 2.0), Stroke::new(1.0, p.stroke));
    // round the ends by masking corners with the card colour
    painter.rect_stroke(track.expand(2.0), Rounding::same(h / 2.0 + 2.0), Stroke::new(4.0, p.card));
    if let Some((lo, hi)) = zone {
        let z = Rect::from_x_y_ranges(
            track.left() + track.width() * lo..=track.left() + track.width() * hi,
            track.top() - 4.0..=track.bottom() + 4.0,
        );
        let pts = [z.left_top(), z.right_top(), z.right_bottom(), z.left_bottom(), z.left_top()];
        painter.extend(Shape::dashed_line(&pts, Stroke::new(1.5, p.text), 5.0, 4.0));
    }
    if let Some(v) = value {
        let c = pos2(track.left() + track.width() * v.clamp(0.0, 1.0), track.center().y);
        painter.circle_filled(c, 11.0, p.card);
        painter.circle_stroke(c, 11.0, Stroke::new(3.0, p.text));
    }
    let y = track.bottom() + 14.0;
    let f = FontId::proportional(12.0);
    painter.text(pos2(track.left(), y), Align2::LEFT_CENTER, "darker", f.clone(), p.muted);
    painter.text(pos2(track.center().x, y), Align2::CENTER_CENTER, "balanced", f.clone(), p.muted);
    painter.text(pos2(track.right(), y), Align2::RIGHT_CENTER, "brighter", f, p.muted);
}

/// Three-step register indicator: Chest | Mix | Head.
pub fn register_meter(ui: &mut Ui, p: &Pal, current: Option<Register>) {
    let labels = [("Chest", Register::Chest), ("Mix", Register::Mix), ("Head / falsetto", Register::Head)];
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
    let gap = 6.0;
    let w = (rect.width() - gap * 2.0) / 3.0;
    let painter = ui.painter_at(rect);
    for (i, (label, reg)) in labels.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * (w + gap), 0.0), vec2(w, rect.height()));
        let on = current == Some(*reg);
        painter.rect_filled(r, Rounding::same(10.0), if on { p.text } else { p.card2 });
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::new(13.5, bold()),
            if on { p.on_text } else { p.muted },
        );
    }
}

const G_MIN: f32 = 36.0; // C2
const G_MAX: f32 = 74.0; // D5
const WINDOW_S: f32 = 12.0;

/// Scrolling pitch graph with goal band, passaggio line and crack markers.
pub fn pitch_graph(
    ui: &mut Ui,
    p: &Pal,
    points: &VecDeque<(f32, Option<f32>)>,
    cracks: &[Crack],
    target: &Target,
    passaggio: Option<f32>,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    let left = rect.left() + 34.0;
    let right = rect.right() - 4.0;
    let (top, bottom) = (rect.top() + 6.0, rect.bottom() - 6.0);
    let y = |hz: f32| {
        let m = hz_to_midi(hz).clamp(G_MIN, G_MAX);
        bottom - (m - G_MIN) / (G_MAX - G_MIN) * (bottom - top)
    };

    // goal band
    let band = Rect::from_x_y_ranges(left..=right, y(target.max_hz)..=y(target.min_hz));
    painter.rect_filled(band, Rounding::same(8.0), p.card2);

    // note grid
    let f = FontId::proportional(10.5);
    for m in G_MIN as i32..=G_MAX as i32 {
        let pc = m.rem_euclid(12);
        if pc != 0 && pc != 4 && pc != 7 {
            continue;
        }
        let yy = y(midi_to_hz(m as f32));
        painter.line_segment([pos2(left, yy), pos2(right, yy)], Stroke::new(if pc == 0 { 1.0 } else { 0.5 }, p.stroke));
        painter.text(pos2(rect.left(), yy), Align2::LEFT_CENTER, midi_name(m), f.clone(), p.muted);
    }

    // passaggio
    if let Some(ph) = passaggio {
        let yy = y(ph);
        painter.extend(Shape::dashed_line(&[pos2(left, yy), pos2(right, yy)], Stroke::new(1.2, p.muted), 6.0, 5.0));
        painter.text(pos2(right - 2.0, yy - 8.0), Align2::RIGHT_CENTER, "passaggio", f.clone(), p.muted);
    }

    let Some(&(now, _)) = points.back() else { return };
    let x = |t: f32| right - (now - t) / WINDOW_S * (right - left);

    // pitch line: thicker inside the goal range, thinner outside
    let mut prev: Option<(f32, f32)> = None;
    for &(t, hz) in points.iter() {
        match hz {
            Some(h) if t >= now - WINDOW_S => {
                if let Some((pt, ph)) = prev {
                    let w = if target.contains(h) { 3.0 } else { 1.6 };
                    painter.line_segment([pos2(x(pt), y(ph)), pos2(x(t), y(h))], Stroke::new(w, p.text));
                }
                prev = Some((t, h));
            }
            _ => prev = None,
        }
    }

    // crack markers
    for c in cracks.iter().filter(|c| c.t >= now - WINDOW_S) {
        let cx = x(c.t);
        let (y1, y2) = (y(c.from_hz), y(c.to_hz));
        painter.line_segment([pos2(cx, y1), pos2(cx, y2)], Stroke::new(1.0, p.muted));
        painter.circle_filled(pos2(cx, y2), 4.5, p.text);
        painter.circle_stroke(pos2(cx, y2), 7.5, Stroke::new(1.0, p.text));
    }
}

/// Chromatic strip E2..C5. Goal notes tinted, current note inverted.
pub fn note_strip(ui: &mut Ui, p: &Pal, current: Option<i32>, target: &Target, passaggio: Option<f32>) {
    let first = 40; // E2
    let last = 72; // C5
    let count = (last - first + 1) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 74.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let gap = 3.0;
    let w = (rect.width() - gap * (count - 1.0)) / count;
    let lo = hz_to_midi(target.min_hz).round() as i32;
    let hi = hz_to_midi(target.max_hz).round() as i32;
    let pm = passaggio.map(|h| hz_to_midi(h).round() as i32);
    for m in first..=last {
        let i = (m - first) as f32;
        let sharp = is_sharp(m);
        let h = if sharp { 50.0 } else { 70.0 };
        let r = Rect::from_min_size(pos2(rect.left() + i * (w + gap), rect.top()), vec2(w, h));
        let on = current == Some(m);
        let in_goal = m >= lo && m <= hi;
        let fill = if on {
            p.text
        } else if in_goal {
            if p.dark { Color32::from_gray(if sharp { 70 } else { 58 }) } else { Color32::from_gray(if sharp { 195 } else { 214 }) }
        } else if sharp {
            p.card2
        } else {
            p.card
        };
        painter.rect_filled(r, Rounding::same(7.0), fill);
        painter.rect_stroke(r, Rounding::same(7.0), Stroke::new(1.0, p.stroke));
        if pm == Some(m) {
            painter.circle_filled(pos2(r.center().x, r.top() + 8.0), 3.0, if on { p.on_text } else { p.text });
        }
        if !sharp && w > 14.0 {
            painter.text(
                pos2(r.center().x, r.bottom() - 10.0),
                Align2::CENTER_CENTER,
                midi_name(m),
                FontId::proportional(if w > 24.0 { 10.5 } else { 8.5 }),
                if on { p.on_text } else { p.muted },
            );
        }
    }
}
