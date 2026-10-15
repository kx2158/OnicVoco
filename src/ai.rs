//! Optional AI coach. Downloads a small model once, then runs it on the CPU
//! with llama.cpp in a background thread. It never receives audio, only the
//! session summary numbers.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

const MODEL_FILE: &str = "qwen2.5-0.5b-instruct-q4_k_m.gguf";
const MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
pub const MODEL_SIZE_MB: u32 = 490;

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

pub fn model_path() -> PathBuf {
    crate::storage::local_dir().join("models").join(MODEL_FILE)
}

pub fn installed() -> bool {
    model_path().exists()
}

pub fn remove() -> std::io::Result<()> {
    let p = model_path();
    if p.exists() {
        std::fs::remove_file(p)?;
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
    #[cfg(feature = "ai")]
    let mut engine: Option<engine::Engine> = None;

    while let Ok(cmd) = cmds.recv() {
        match cmd {
            Cmd::Download => match download(&send) {
                Ok(()) => send(Msg::Installed),
                Err(e) => send(Msg::Error(format!("Download failed: {e}"))),
            },
            Cmd::Generate { system, user } => {
                #[cfg(feature = "ai")]
                {
                    if engine.is_none() {
                        send(Msg::Progress(0.0, "Loading model…".into()));
                        match engine::Engine::load(&model_path()) {
                            Ok(e) => engine = Some(e),
                            Err(e) => {
                                send(Msg::Error(format!("Couldn't load the model: {e}")));
                                continue;
                            }
                        }
                    }
                    let result = engine.as_ref().unwrap().generate(&system, &user, |t| send(Msg::Token(t)));
                    match result {
                        Ok(()) => send(Msg::Done),
                        Err(e) => send(Msg::Error(format!("The AI coach couldn't run: {e}"))),
                    }
                }
                #[cfg(not(feature = "ai"))]
                {
                    let _ = (system, user);
                    send(Msg::Error("This build was made without the AI coach.".into()));
                }
            }
        }
    }
}

fn download(send: &impl Fn(Msg)) -> Result<(), String> {
    use std::io::{Read, Write};
    let path = model_path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let part = path.with_extension("part");
    let resp = ureq::get(MODEL_URL).call().map_err(|e| e.to_string())?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    let mut done: u64 = 0;
    let mut last_report = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if done - last_report > 2_000_000 {
            last_report = done;
            let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
            send(Msg::Progress(frac, format!("{} / {} MB", done / 1_000_000, total / 1_000_000)));
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if total > 0 && done != total {
        let _ = std::fs::remove_file(&part);
        return Err("the download was interrupted".into());
    }
    std::fs::rename(&part, &path).map_err(|e| e.to_string())
}

#[cfg(feature = "ai")]
mod engine {
    use anyhow::{anyhow, Result};
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;
    use std::num::NonZeroU32;
    use std::path::Path;

    pub struct Engine {
        backend: LlamaBackend,
        model: LlamaModel,
    }

    impl Engine {
        pub fn load(path: &Path) -> Result<Self> {
            let backend = LlamaBackend::init()?;
            let model = LlamaModel::load_from_file(&backend, path, &LlamaModelParams::default())?;
            Ok(Engine { backend, model })
        }

        fn prompt(&self, system: &str, user: &str) -> String {
            let templated = (|| -> Result<String> {
                let tmpl = self.model.chat_template(None)?;
                let msgs = vec![
                    LlamaChatMessage::new("system".into(), system.into())?,
                    LlamaChatMessage::new("user".into(), user.into())?,
                ];
                Ok(self.model.apply_chat_template(&tmpl, &msgs, true)?)
            })();
            templated.unwrap_or_else(|_| {
                format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n")
            })
        }

        pub fn generate(&self, system: &str, user: &str, mut on_token: impl FnMut(String)) -> Result<()> {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32;
            let params = LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(2048))
                .with_n_threads(threads)
                .with_n_threads_batch(threads);
            let mut ctx = self.model.new_context(&self.backend, params)?;

            let tokens = self.model.str_to_token(&self.prompt(system, user), AddBos::Never)?;
            if tokens.is_empty() || tokens.len() > 1500 {
                return Err(anyhow!("prompt has an unexpected length"));
            }
            let mut batch = LlamaBatch::new(2048, 1);
            let last = tokens.len() as i32 - 1;
            for (i, tok) in (0_i32..).zip(tokens.into_iter()) {
                batch.add(tok, i, &[0], i == last)?;
            }
            ctx.decode(&mut batch)?;

            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::temp(0.5),
                LlamaSampler::top_p(0.9, 1),
                LlamaSampler::dist(1234),
            ]);
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            let mut n_cur = batch.n_tokens();
            let max = n_cur + 220;
            while n_cur < max {
                let token = sampler.sample(&ctx, batch.n_tokens() - 1);
                sampler.accept(token);
                if self.model.is_eog_token(token) {
                    break;
                }
                let piece = self.model.token_to_piece(token, &mut decoder, true, None)?;
                on_token(piece.replace('—', " - "));
                batch.clear();
                batch.add(token, n_cur, &[0], true)?;
                n_cur += 1;
                ctx.decode(&mut batch)?;
            }
            Ok(())
        }
    }
}
