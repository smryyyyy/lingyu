use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

pub struct Recorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
    channels: u16,
}

impl Recorder {
    pub fn new() -> Self {
        let (sample_rate, channels) = Self::probe_input().unwrap_or((44100, 1));
        Self { samples: Arc::new(Mutex::new(Vec::new())), stream: None, sample_rate, channels }
    }

    pub fn input_available() -> bool { Self::probe_input().is_some() }

    fn probe_input() -> Option<(u32, u16)> {
        let host = cpal::default_host();
        let device = host.default_input_device()?;
        let config = device.default_input_config().ok()?;
        Some((config.sample_rate().0, config.channels()))
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("没有输入设备")?;
        let config = device.default_input_config().map_err(|e| format!("配置错误：{e}"))?;
        self.sample_rate = config.sample_rate().0;
        self.channels = config.channels();
        let samples = Arc::clone(&self.samples);
        samples.lock().expect("poisoned").clear();
        let err_fn = |err| eprintln!("音频流错误：{err}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let s = Arc::clone(&samples);
                device.build_input_stream(&config.into(),
                    move |data: &[f32], _| { s.lock().expect("poisoned").extend_from_slice(data); },
                    err_fn, None
                ).map_err(|e| format!("构建流失败：{e}"))?
            }
            cpal::SampleFormat::I16 => {
                let s = Arc::clone(&samples);
                device.build_input_stream(&config.into(),
                    move |data: &[i16], _| {
                        s.lock().expect("poisoned").extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                    }, err_fn, None
                ).map_err(|e| format!("构建流失败：{e}"))?
            }
            cpal::SampleFormat::U16 => {
                let s = Arc::clone(&samples);
                device.build_input_stream(&config.into(),
                    move |data: &[u16], _| {
                        s.lock().expect("poisoned").extend(data.iter().map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0));
                    }, err_fn, None
                ).map_err(|e| format!("构建流失败：{e}"))?
            }
            fmt => return Err(format!("不支持的格式：{fmt:?}")),
        };
        stream.play().map_err(|e| format!("播放失败：{e}"))?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<Vec<u8>, String> {
        self.stream.take();
        let samples = self.samples.lock().map_err(|_| "缓冲区锁破坏".to_string())?;
        if samples.is_empty() { return Err("未录制到音频".into()); }

        let mono: Vec<f32> = if self.channels > 1 {
            samples.chunks(self.channels as usize).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect()
        } else { samples.clone() };

        let mut buf = Cursor::new(Vec::new());
        let spec = hound::WavSpec { channels: 1, sample_rate: self.sample_rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::new(&mut buf, spec).map_err(|e| format!("WAV 写入错误：{e}"))?;
        for &sample in &mono {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .map_err(|e| format!("WAV 样本错误：{e}"))?;
        }
        writer.finalize().map_err(|e| format!("WAV 完成错误：{e}"))?;
        Ok(buf.into_inner())
    }

    pub fn sample_rate(&self) -> u32 { self.sample_rate }
}
