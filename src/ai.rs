//! Optional AI coach.
//!
//! Nothing AI related ships inside OnicVoco.exe. When the user asks for it,
//! the app downloads a small helper program (onicvoco-coach.exe, built from
//! the `coach/` folder and attached to the GitHub release) plus the language
//! model. To get feedback it starts the helper in the background, sends it the
//! session numbers on stdin and reads the answer back. The helper never
//! receives audio.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};

const MODEL_FILE: &str = "qwen2.5-0.5b-instruct-q4_k_m.gguf";
const MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
const RELEASE_BASE: &str = "https://github.com/kx2158/OnicVoco/releases/latest/download/";
const HELPER_EXE: &str = "onicvoco-coach.exe";

/// (asset name on the release, file name on disk)
const HELPER_FILES: &[(&str, &str)] = &[
    ("onicvoco-coach.exe", "onicvoco-coach.exe"),
    ("coach-vcruntime140.dll", "vcruntime140.dll"),
    ("coach-vcruntime140_1.dll", "vcruntime140_1.dll"),
    ("coach-msvcp140.dll", "msvcp140.dll"),
];

pub const DOWNLOAD_SIZE_MB: u32 = 500;

pub enum Msg {
    Progress(f32, String),
    Installed,
    Token(String),
    Done,
    Error(String),
}

enum Cmd {
    Download,
    Generate { system: String, user: String },
}

pub struct AiCoach {
    tx: Sender<Cmd>,
    pub rx: Receiver<Msg>,
}

fn coach_dir() -> PathBuf {
    crate::storage::local_dir().join("coach")
}

fn model_path() -> PathBuf {
    coach_dir().join(MODEL_FILE)
}

fn helper_path() -> PathBuf {
    coach_dir().join(HELPER_EXE)
}

pub fn installed() -> bool {
    helper_path().exists() && model_path().exists()
}

pub fn remove() -> std::io::Result<()> {
    let d = coach_dir();
    if d.exists() {
        std::fs::remove_dir_all(d)?;
    }
    Ok(())
}

impl AiCoach {
    pub fn spawn(repaint: eframe::egui::Context) -> Self {
        let (tx, cmd_rx) = channel::<Cmd>();
        let (msg_tx, rx) = channel::<Msg>();
        std::thread::Builder::new()
            .name("ai-coach".into())
            .spawn(move || worker(cmd_rx, msg_tx, repaint))
            .expect("spawn ai thread");
        AiCoach { tx, rx }
    }

    pub fn download(&self) {
        let _ = self.tx.send(Cmd::Download);
    }

    pub fn generate(&self, system: &str, user: &str) {
        let _ = self.tx.send(Cmd::Generate { system: system.into(), user: user.into() });
    }
}

fn worker(cmds: Receiver<Cmd>, out: Sender<Msg>, ctx: eframe::egui::Context) {
    let send = |m: Msg| {
        let _ = out.send(m);
        ctx.request_repaint();
    };
    while let Ok(cmd) = cmds.recv() {
        match cmd {
            Cmd::Download => match download_all(&send) {
                Ok(()) => send(Msg::Installed),
                Err(e) => send(Msg::Error(format!("Download failed: {e}"))),
            },
            Cmd::Generate { system, user } => match run_helper(&system, &user, &send) {
                Ok(()) => send(Msg::Done),
                Err(e) => send(Msg::Error(format!("The AI coach couldn't run: {e}"))),
            },
        }
    }
}

fn download_all(send: &impl Fn(Msg)) -> Result<(), String> {
    std::fs::create_dir_all(coach_dir()).map_err(|e| e.to_string())?;
    let n = HELPER_FILES.len() + 1;
    for (i, (asset, local)) in HELPER_FILES.iter().enumerate() {
        let label = format!("coach helper ({}/{n})", i + 1);
        download(&format!("{RELEASE_BASE}{asset}"), &coach_dir().join(local), &label, send)?;
    }
    download(MODEL_URL, &model_path(), &format!("language model ({n}/{n})"), send)
}

fn download(url: &str, path: &std::path::Path, label: &str, send: &impl Fn(Msg)) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let part = path.with_extension("part");
    let resp = ureq::get(url).call().map_err(|e| match e {
        ureq::Error::Status(404, _) => format!(
            "the {label} isn't available yet. The OnicVoco release on GitHub needs to be published first."
        ),
        other => other.to_string(),
    })?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    let (mut done, mut last) = (0u64, 0u64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if done - last > 1_000_000 {
            last = done;
            let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
            send(Msg::Progress(frac, format!("{label}: {} / {} MB", done / 1_000_000, total / 1_000_000)));
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if total > 0 && done != total {
        let _ = std::fs::remove_file(&part);
        return Err("the download was interrupted".into());
    }
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

fn run_helper(system: &str, user: &str, send: &impl Fn(Msg)) -> Result<(), String> {
    if !installed() {
        return Err("it isn't installed. Download it in Settings.".into());
    }
    send(Msg::Progress(0.0, "Thinking…".into()));
    let mut cmd = Command::new(helper_path());
    cmd.arg("--model")
        .arg(model_path())
        .current_dir(coach_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start the helper ({e})"))?;

    let request = serde_json::json!({ "system": system, "user": user }).to_string();
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    } // closing stdin tells the helper the request is complete

    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut error = None;
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        if let Some(t) = v["token"].as_str() {
            send(Msg::Token(t.replace('—', " - ")));
        } else if let Some(e) = v["error"].as_str() {
            error = Some(e.to_string());
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    match error {
        Some(e) => Err(e),
        None if !status.success() => Err(format!("the helper stopped unexpectedly ({status})")),
        None => Ok(()),
    }
}
