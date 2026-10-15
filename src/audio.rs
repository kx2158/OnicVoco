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
    /// Recording into memory is off until `set_recording(true)`.
    pub fn start(device: Option<&str>) -> Result<Self, String> {
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
            recording: None,
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

    /// Starts (fresh) or stops keeping samples in memory.
    pub fn set_recording(&self, on: bool) {
        if let Ok(mut s) = self.shared.lock() {
            s.recording = on.then(|| Vec::with_capacity(self.sample_rate as usize * 120));
        }
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

/// Plays a mono recording on the default output device.
pub struct Player {
    _stream: cpal::Stream,
    /// Current position in source samples, 16.16 fixed point.
    pos: std::sync::Arc<std::sync::atomic::AtomicU64>,
    finished: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub source_rate: f32,
    pub len: usize,
}

impl Player {
    pub fn start(samples: std::sync::Arc<Vec<f32>>, source_rate: u32, from_secs: f32) -> Result<Self, String> {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("No speakers or headphones found.")?;
        let config = dev.default_output_config().map_err(|e| e.to_string())?;
        let out_rate = config.sample_rate().0 as f64;
        let channels = config.channels() as usize;
        let format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();
        let step = ((source_rate as f64 / out_rate) * 65536.0) as u64;
        let start = ((from_secs.max(0.0) as f64 * source_rate as f64) as u64).min(samples.len() as u64) << 16;
        let pos = std::sync::Arc::new(AtomicU64::new(start));
        let finished = std::sync::Arc::new(AtomicBool::new(false));
        let len = samples.len();

        let (p2, f2, s2) = (pos.clone(), finished.clone(), samples.clone());
        let mut next = move || -> f32 {
            let fixed = p2.load(Ordering::Relaxed);
            let i = (fixed >> 16) as usize;
            if i + 1 >= s2.len() {
                f2.store(true, Ordering::Relaxed);
                return 0.0;
            }
            let frac = (fixed & 0xFFFF) as f32 / 65536.0;
            p2.store(fixed + step, Ordering::Relaxed);
            s2[i] * (1.0 - frac) + s2[i + 1] * frac
        };
        let err = |e: cpal::StreamError| eprintln!("playback error: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => dev.build_output_stream(
                &stream_config,
                move |d: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for frame in d.chunks_mut(channels.max(1)) {
                        let v = next();
                        frame.iter_mut().for_each(|x| *x = v);
                    }
                },
                err,
                None,
            ),
            cpal::SampleFormat::I16 => dev.build_output_stream(
                &stream_config,
                move |d: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    for frame in d.chunks_mut(channels.max(1)) {
                        let v = (next().clamp(-1.0, 1.0) * 32767.0) as i16;
                        frame.iter_mut().for_each(|x| *x = v);
                    }
                },
                err,
                None,
            ),
            other => return Err(format!("Unsupported speaker sample format: {other:?}")),
        }
        .map_err(|e| format!("Could not open the speakers: {e}"))?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Player { _stream: stream, pos, finished, source_rate: source_rate as f32, len })
    }

    pub fn position_secs(&self) -> f32 {
        let i = (self.pos.load(std::sync::atomic::Ordering::Relaxed) >> 16) as usize;
        i.min(self.len) as f32 / self.source_rate
    }

    pub fn finished(&self) -> bool {
        self.finished.load(std::sync::atomic::Ordering::Relaxed)
    }
}
