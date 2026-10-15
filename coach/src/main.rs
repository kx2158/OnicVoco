//! OnicVoco AI coach helper.
//!
//! A separate, optional download. The main app starts it in the background,
//! writes one JSON request to stdin ({"system": "...", "user": "..."}) and
//! reads JSON lines from stdout:
//!   {"token": "..."}   a piece of the answer
//!   {"done": true}     finished
//!   {"error": "..."}   something went wrong
//!
//! Usage: onicvoco-coach --model <path to .gguf>

mod engine;

use std::io::{Read, Write};

fn emit(v: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("onicvoco-coach {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let Some(model) = args.iter().position(|a| a == "--model").and_then(|i| args.get(i + 1)) else {
        emit(serde_json::json!({ "error": "missing --model <path>" }));
        std::process::exit(2);
    };

    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        emit(serde_json::json!({ "error": format!("couldn't read request: {e}") }));
        std::process::exit(1);
    }
    let req: serde_json::Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            emit(serde_json::json!({ "error": format!("bad request: {e}") }));
            std::process::exit(1);
        }
    };
    let system = req["system"].as_str().unwrap_or_default();
    let user = req["user"].as_str().unwrap_or_default();

    let result = engine::Engine::load(std::path::Path::new(model))
        .and_then(|e| e.generate(system, user, |t| emit(serde_json::json!({ "token": t }))));
    match result {
        Ok(()) => emit(serde_json::json!({ "done": true })),
        Err(e) => {
            emit(serde_json::json!({ "error": e.to_string() }));
            std::process::exit(1);
        }
    }
}
