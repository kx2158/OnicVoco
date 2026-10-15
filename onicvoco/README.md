# OnicVoco

Gentle, private voice training for Windows, macOS and Linux.

- **Live pitch meter** with the frequency in Hz and the note in scientific pitch notation (G3 +4¢)
- **Goal ranges**: feminine, androgynous, masculine or your own custom range
- **Resonance meter** (darker to brighter), estimated from your formants
- **Pitch-over-time graph** and a note strip that highlights your goal range
- **Microphone picker** and input level meter
- **Session summary** with tips, playback and an option to save your recording
- **Progress history** stored only on your device
- **Optional AI coach** that turns your session numbers into friendly feedback

Everything runs locally. Your voice is never uploaded anywhere.

## About the AI coach

The AI coach is **off by default** and nothing AI related is included in the installer.
If someone wants it, they can download it from Settings:

- Model: Qwen2.5 0.5B Instruct (8-bit), about 520 MB, from Hugging Face
- Runs entirely on the computer's CPU after the one-time download
- It never hears audio. It only reads the numbers from the session summary (median pitch, % in range, resonance, and so on). The app does all the measuring itself.
- "Remove model" in Settings deletes it completely

The basic, rule-based tips work without the AI coach.

## How the measurements work

| What | How |
|---|---|
| Pitch | YIN algorithm on 2048-sample frames (~30 times per second), median-smoothed to avoid octave jumps |
| Notes | A4 = 440 Hz, middle C = C4, cents = distance from the nearest note |
| Resonance | LPC analysis finds the first two formants (F1, F2), which are mapped to a 0 (darker) to 1 (brighter) score |

Resonance depends heavily on which vowel you are saying, so treat it as a trend over a few sentences, not a verdict on a single word.

Pitch ranges are common rough guides from voice training communities:

| Goal | Range | Notes |
|---|---|---|
| Masculine | 85-155 Hz | F2 to D#3 |
| Androgynous | 145-175 Hz | D3 to F3 |
| Feminine | 165-255 Hz | E3 to C4 |

Every voice is different, so the custom range is there for a reason.

## Development

You need [Node.js](https://nodejs.org) 20+ and [Rust](https://rustup.rs). On Linux, also install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm test               # analysis unit tests
npm run dev            # UI only, in your browser at http://localhost:1420
npm run tauri dev      # the real desktop app
npm run tauri build    # installers for your current OS
```

## Releases on GitHub

`.github/workflows/release.yml` builds every platform on GitHub's servers:

1. Bump the version in `package.json` and `src-tauri/tauri.conf.json`
2. Commit, then tag and push:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. When the workflow finishes, open **Releases**, check the draft and publish it

You can also start it manually from the **Actions** tab (Release > Run workflow).

Builds produced:

| Platform | Files |
|---|---|
| Windows | `.msi`, `-setup.exe` |
| macOS Apple Silicon | `aarch64.dmg` |
| macOS Intel | `x64.dmg` |
| Linux | `.AppImage`, `.deb`, `.rpm` |

### macOS note

The macOS builds are ad-hoc signed but not notarized (that needs a paid Apple Developer account). The first time, users should right-click the app and choose **Open**. If macOS says the app "is damaged", run this once in Terminal:

```bash
xattr -cr /Applications/OnicVoco.app
```

### Microphone permissions

- **macOS**: asks the first time. If it was denied: System Settings > Privacy & Security > Microphone
- **Windows**: Settings > Privacy & security > Microphone > allow desktop apps
- **Linux**: needs GStreamer's PulseAudio/PipeWire plugin, bundled in the AppImage and listed as a dependency of the `.deb`

## Project layout

```
src/
  audio/pitch.js      YIN pitch detection
  audio/formants.js   LPC formants + resonance score
  audio/engine.js     microphone capture and analysis loop
  notes.js            scientific pitch notation
  targets.js          goal ranges
  coach.js            session stats, tips and the AI prompt
  ai/                 optional AI coach (web worker + model storage)
  ui/                 pitch graph and note strip
src-tauri/            desktop shell (Rust, Tauri 2)
tests/                unit tests with synthetic voices
```
