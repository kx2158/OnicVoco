
use anyhow::{anyhow, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
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

        let vocab = self.model.vocab();
        // Qwen uses no BOS token; parse the chat template's special tokens.
        let tokens = vocab.tokenize(self.prompt(system, user).as_bytes(), false, true);
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
            if vocab.is_eog(token) {
                break;
            }
            // Tokens can split multi-byte characters, so decode as a stream.
            let bytes = vocab.token_to_piece(token, false, None);
            let cap = decoder.max_utf8_buffer_length(bytes.len()).unwrap_or(bytes.len() * 3 + 8);
            let mut piece = String::with_capacity(cap);
            let _ = decoder.decode_to_string(&bytes, &mut piece, false);
            if !piece.is_empty() {
                on_token(piece.replace('—', " - "));
            }
            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            n_cur += 1;
            ctx.decode(&mut batch)?;
        }
        Ok(())
    }
}
