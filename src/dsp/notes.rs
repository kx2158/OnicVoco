//! Scientific pitch notation (A4 = 440 Hz, middle C = C4).

pub const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

pub fn midi_name(midi: i32) -> String {
    format!("{}{}", NAMES[midi.rem_euclid(12) as usize], midi.div_euclid(12) - 1)
}

pub fn is_sharp(midi: i32) -> bool {
    NAMES[midi.rem_euclid(12) as usize].contains('#')
}

#[derive(Debug, Clone)]
pub struct Note {
    pub name: String,
    pub midi: i32,
    pub cents: i32,
}

pub fn hz_to_note(hz: f32) -> Note {
    let exact = hz_to_midi(hz);
    let midi = exact.round() as i32;
    Note { name: midi_name(midi), midi, cents: ((exact - midi as f32) * 100.0).round() as i32 }
}

pub fn note_name(hz: f32) -> String {
    hz_to_note(hz).name
}

pub fn format_cents(c: i32) -> String {
    match c {
        0 => "±0¢".into(),
        c if c > 0 => format!("+{c}¢"),
        c => format!("−{}¢", -c),
    }
}

pub fn semitones(a: f32, b: f32) -> f32 {
    12.0 * (b / a).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(note_name(440.0), "A4");
        assert_eq!(note_name(261.63), "C4");
        assert_eq!(note_name(196.0), "G3");
        assert_eq!(note_name(82.41), "E2");
        assert_eq!(hz_to_note(445.0).cents, 20);
        assert_eq!(format_cents(-12), "−12¢");
        assert!(is_sharp(61));
    }
}
