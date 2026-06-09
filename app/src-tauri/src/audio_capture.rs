use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

const TARGET_SAMPLE_RATE: u32 = 16000;

pub struct AudioCapture {
    pcm: Arc<Mutex<Vec<i16>>>,
    _stream: cpal::Stream,
}

impl AudioCapture {
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "未找到默认麦克风".to_string())?;
        let config = device
            .default_input_config()
            .map_err(|e| e.to_string())?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let pcm: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
        let pcm_cb = Arc::clone(&pcm);

        let err_fn = |err| tracing::error!("audio stream error: {err}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        append_samples(data, sample_rate, channels, &pcm_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?,
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        append_i16(data, sample_rate, channels, &pcm_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?,
            other => return Err(format!("unsupported sample format: {other:?}")),
        };

        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            pcm,
            _stream: stream,
        })
    }

    pub fn pcm_buffer(&self) -> Arc<Mutex<Vec<i16>>> {
        Arc::clone(&self.pcm)
    }

    pub fn drain_new_pcm_bytes(&self) -> Vec<u8> {
        self.drain_all_pcm_bytes()
    }

    pub fn drain_all_pcm_bytes(&self) -> Vec<u8> {
        let samples = std::mem::take(&mut *self.pcm.lock().unwrap());
        samples
            .into_iter()
            .flat_map(|s| s.to_le_bytes())
            .collect()
    }
}

fn append_samples(input: &[f32], sample_rate: u32, channels: usize, out: &Arc<Mutex<Vec<i16>>>) {
    if channels == 0 {
        return;
    }
    let mono: Vec<f32> = if channels == 1 {
        input.to_vec()
    } else {
        input
            .chunks(channels)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    let resampled = resample_to_16k(&mono, sample_rate);
    let mut guard = out.lock().unwrap();
    guard.extend(resampled.into_iter().map(|f| (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16));
}

fn append_i16(input: &[i16], sample_rate: u32, channels: usize, out: &Arc<Mutex<Vec<i16>>>) {
    if channels == 0 {
        return;
    }
    let mono: Vec<f32> = if channels == 1 {
        input.iter().map(|&s| s as f32 / i16::MAX as f32).collect()
    } else {
        input
            .chunks(channels)
            .map(|c| c.iter().map(|&s| s as f32).sum::<f32>() / channels as f32 / i16::MAX as f32)
            .collect()
    };
    let resampled = resample_to_16k(&mono, sample_rate);
    let mut guard = out.lock().unwrap();
    guard.extend(resampled.into_iter().map(|f| (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16));
}

fn resample_to_16k(input: &[f32], sample_rate: u32) -> Vec<f32> {
    if sample_rate == TARGET_SAMPLE_RATE || input.is_empty() {
        return input.to_vec();
    }
    let ratio = sample_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len.max(1));
    for i in 0..out_len {
        let src = (i as f64 * ratio) as usize;
        out.push(input[src.min(input.len() - 1)]);
    }
    out
}
