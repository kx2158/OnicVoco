//! Native microphone capture with cpal (WASAPI on Windows).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const RING: usize = 16384;

#[derive(Default)]
struct Shared {
    ring: VecDeque<f32>,
    recording: Option<Vec<f32>>,
    peak_error: Option<String>,
}

pub struct AudioInput {
    _stream: cpal::Stream,
    pub sample_rate: f32,
    pub device_name: String,
    shared: Arc<Mutex<Shared>>,
}

pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

impl AudioInput {
    /// Opens `device` (or the default input) and starts capturing.
    pub fn start(device: Option<&str>, record: bool) -> Result<Self, String> {
        let host = cpal::default_host();
        let dev = match device {
            Some(name) => host
                .input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .or_else(|| host.default_input_device()),
            None => host.default_input_device(),
        }
        .ok_or("No microphone found. Plug one in or check Windows sound settings.")?;
        let device_name = dev.name().unwrap_or_else(|_| "Microphone".into());
        let config = dev.default_input_config().map_err(|e| e.to_string())?;
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();

        let shared = Arc::new(Mutex::new(Shared {
            ring: VecDeque::with_capacity(RING),
            recording: record.then(Vec::new),
            peak_error: None,
        }));

        let err_shared = shared.clone();
        let on_err = move |e: cpal::StreamError| {
            if let Ok(mut s) = err_shared.lock() {
                s.peak_error = Some(e.to_string());
            }
        };

        fn push<T: Copy>(shared: &Arc<Mutex<Shared>>, data: &[T], channels: usize, conv: impl Fn(T) -> f32) {
            let Ok(mut s) = shared.lock() else { return };
            for frame in data.chunks(channels.max(1)) {
                let v = frame.iter().map(|&x| conv(x)).sum::<f32>() / frame.len() as f32;
                if s.ring.len() >= RING {
                    s.ring.pop_front();
                }
                s.ring.push_back(v);
                if let Some(rec) = s.recording.as_mut() {
                    rec.push(v);
                }
            }
        }

        let sh = shared.clone();
        let stream = match format {
            cpal::SampleFormat::F32 => dev.build_input_stream(
                &stream_config,
                move |d: &[f32], _: &cpal::InputCallbackInfo| push(&sh, d, channels, |x| x),
                on_err,
                None,
            ),
            cpal::SampleFormat::I16 => dev.build_input_stream(
                &stream_config,
                move |d: &[i16], _: &cpal::InputCallbackInfo| push(&sh, d, channels, |x| x as f32 / 32768.0),
                on_err,
                None,
            ),
            cpal::SampleFormat::U16 => dev.build_input_stream(
                &stream_config,
                move |d: &[u16], _: &cpal::InputCallbackInfo| push(&sh, d, channels, |x| (x as f32 - 32768.0) / 32768.0),
                on_err,
                None,
            ),
            cpal::SampleFormat::I32 => dev.build_input_stream(
                &stream_config,
                move |d: &[i32], _: &cpal::InputCallbackInfo| push(&sh, d, channels, |x| x as f32 / 2147483648.0),
                on_err,
                None,
            ),
            other => return Err(format!("Unsupported microphone sample format: {other:?}")),
        }
        .map_err(|e| format!("Could not open the microphone: {e}"))?;
        stream.play().map_err(|e| format!("Could not start the microphone: {e}"))?;

        Ok(AudioInput { _stream: stream, sample_rate, device_name, shared })
    }

    /// Copies the newest `n` samples into `out`. False if not enough yet.
    pub fn latest(&self, n: usize, out: &mut Vec<f32>) -> bool {
        let Ok(s) = self.shared.lock() else { return false };
        if s.ring.len() < n {
            return false;
        }
        out.clear();
        out.extend(s.ring.iter().skip(s.ring.len() - n));
        true
    }

    pub fn error(&self) -> Option<String> {
        self.shared.lock().ok().and_then(|s| s.peak_error.clone())
    }

    pub fn take_recording(&self) -> Option<Vec<f32>> {
        self.shared.lock().ok().and_then(|mut s| s.recording.take())
    }
}

/// Writes mono samples to a 16-bit WAV file.
pub fn save_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}
