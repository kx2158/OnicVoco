# OnicVoco

Gentle, private voice training. A fully native Windows app written in Rust: no browser, no webview, one small `.exe`.

## Features

- **Always-live mic** with a **peak meter** (peak hold + "too loud" warning), so you see pitch and level without recording
- **Live pitch** in Hz and scientific pitch notation (G3 +4¢), with a status for your goal range
- **Goals**: feminine, androgynous, masculine or a custom range
- **Register meter**: chest, mix / passaggio, head / falsetto (estimated from H1-H2)
- **Find my passaggio**: glide low to high and the app finds where your voice changes gear, then marks it on the graph and note strip
- **Voice crack detection**: sudden jumps between falsetto and chest (either direction) are marked on the pitch graph with a little pop, counted in your summary, with tips right when they happen
- **Resonance meter** (darker to brighter), estimated from formants
- **Undertones monitor**: a live mini-spectrum around your pitch plus meters for airiness, vocal fry and roughness (sub-harmonics at half your pitch)
- **Range tab**: a vocal range map that fills in while the mic is live (time on every note from C2 to C6, stacked by chest / mix / head, with crack spots, comfort zone and goal coverage), plus a guided **Map my range** glide
- **Voice web** (Progress tab): a radar chart that grows as you use the app and shows your strengths and the area with the most room to grow: pitch, steadiness, resonance, smooth breaks, clean tone, range and practice habit
- **Read tab**: practice passages and quick phrases with a live meter on top
- **Learn tab**: chest voice, head voice and falsetto, passaggio, mix, resonance, voice cracks, safe practice, with exercises
- **Pitch graph** with very light semitone lines, a "Last 12 s" or "Whole session" view
- **Session summary** showing the whole recording fitted to the width, with in-app **playback** and a playhead (click to jump), WAV saving, and **progress history**, all stored only on your computer
- **Optional AI coach** that turns your session numbers into friendly feedback
- Monochrome light and dark themes (follows Windows by default), smooth animations, Bahnschrift + Segoe UI
- There may be a small someone living in the top-left corner

Press **Space** to start or finish a recording.

## The AI coach

Off by default, and nothing AI related is inside OnicVoco.exe or the installer. It's a separate download, fetched only when someone presses **Download AI coach** in **Settings**:

- A small helper program, `onicvoco-coach.exe` (built from `coach/`, attached to each GitHub release)
- Qwen2.5 0.5B Instruct (Q4_K_M GGUF, about 490 MB) from Hugging Face
- After that it runs natively on the CPU with llama.cpp, fully offline. The app starts the helper in the background only when you ask for feedback
- It never hears audio. It only reads the numbers from the session summary. All measuring is done by the app itself.
- **Remove model** deletes it

## How the measurements work

| What | How |
|---|---|
| Pitch | YIN on 2048-sample frames, ~30 per second, median-smoothed |
| Notes | A4 = 440 Hz, middle C = C4 |
| Register | H1-H2: level difference between the 1st and 2nd harmonic. Low = chest, high = head/falsetto |
| Cracks | The median pitch a few frames before vs. after a break differs by 4.5+ semitones (up or down), allowing a transitional frame or a tiny dropout at the break; breathy falsetto frames are still tracked |
| Resonance | LPC formant analysis (F1, F2) mapped to a darker-to-brighter score |
| Airy | How periodic the waveform is: breathier frames are less periodic |
| Fry | Pitch below about 80 Hz, or a slow pulse train under the normal range when no regular pitch is found |
| Rough | Level at half the pitch relative to the pitch (sub-harmonics). Needs a pitch of 130 Hz or more, because mains hum sits near 50 Hz |
| Range map | Every analysis frame adds its duration to the nearest semitone; a note counts as in your range after 0.4 s in total |
| Voice web | Each axis is a running average of something the app already measures, with a gentle memory (recent practice counts a bit more). A crack counts against *Smooth breaks* as ten seconds of rough going. Range and habit come from the range map and practice days |

Register and resonance are estimates. They shift with vowels and microphones, so watch the trend.

## Download

Go to **Releases** and grab either:
- `OnicVoco-x.y.z-setup.exe` - installer, no admin rights needed
- `OnicVoco-x.y.z-portable.exe` - just run it

The app isn't code-signed yet, so Windows SmartScreen may warn you. Click **More info > Run anyway**.

## Building

Install [Rust](https://rustup.rs) and Visual Studio Build Tools (C++ workload). The AI helper also needs CMake and LLVM.

```powershell
cargo run --release -p onicvoco             # run the app
cargo test -p onicvoco                       # analysis unit tests
# the optional AI helper (llama.cpp needs the dynamic C runtime):
$env:RUSTFLAGS="-C target-feature=-crt-static"; cargo build --release -p onicvoco-coach --target-dir target-coach
```

## Releasing

Actions tab > **Release** > **Run workflow**, enter a version like `0.2.0`. It builds the installer and portable exe and creates a draft release. Check it, then run **Publish release** with the same version to make it public and the latest (the AI coach download uses the latest release).

## Project layout

```
src/
  main.rs        window setup
  app.rs         the UI: Practice, Read, Learn, Range, Progress, Settings
  widgets.rs     pitch graph, note strip, meters, range map, voice web
  theme.rs       monochrome themes + Segoe UI
  audio.rs       microphone capture (WASAPI via cpal), WAV saving
  analyzer.rs    turns audio into analysis frames
  dsp/           pitch, formants, register, cracks, undertones, FFT, notes (unit tested)
  session.rs     stats, tips, passaggio finder, AI prompt
  profile.rs     range map and voice web (kept on this computer)
  content.rs     reading passages and training guides
  ai.rs          optional AI coach: downloads and runs the helper
coach/           the separate AI helper program (llama.cpp)
installer/       Inno Setup script
```
