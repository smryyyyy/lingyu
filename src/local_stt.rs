use std::path::Path;
use std::process::Command;

/// Local speech-to-text using FunASR llama.cpp runtime (SenseVoice GGUF).
#[derive(Clone)]
pub struct LocalWhisper {
    binary_path: String,
    model_path: String,
    vad_path: Option<String>,
}

impl LocalWhisper {
    pub fn new(bin_dir: &Path, model_path: &Path, models_dir: &Path) -> Result<Self, String> {
        let binary_path = bin_dir.join(crate::config::FUNASR_BINARY.binary_name);
        let vad_path = models_dir.join(crate::config::VAD_MODEL_FILENAME);

        Ok(Self {
            binary_path: binary_path.to_string_lossy().to_string(),
            model_path: model_path.to_string_lossy().to_string(),
            vad_path: if vad_path.exists() { Some(vad_path.to_string_lossy().to_string()) } else { None },
        })
    }

    pub fn transcribe(&self, wav_data: &[u8], _sample_rate: u32) -> Result<String, String> {
        if !Path::new(&self.binary_path).exists() {
            return Err("FunASR 二进制未找到".into());
        }
        if !Path::new(&self.model_path).exists() {
            return Err("模型文件未找到".into());
        }

        let temp_dir = std::env::temp_dir();
        let wav_path = temp_dir.join("lingyu_input.wav");
        std::fs::write(&wav_path, wav_data).map_err(|e| format!("写入临时文件失败：{e}"))?;

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("-m").arg(&self.model_path).arg("-a").arg(&wav_path);
        // Q8 model outputs clean text; F16 needs --keep-tags otherwise stdout is empty
        if self.model_path.contains("f16") {
            cmd.arg("--keep-tags");
        }
        if let Some(ref vad) = self.vad_path { cmd.arg("--vad").arg(vad); }

        let output = cmd.output().map_err(|e| format!("启动 FunASR 失败：{e}"))?;
        let _ = std::fs::remove_file(&wav_path);

        if !output.status.success() {
            return Err(format!("FunASR 识别失败：{}", String::from_utf8_lossy(&output.stderr)));
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        // Strip SenseVoice special tags like <|nospeech|>, <|zh|>, <|en|>, etc.
        let mut text = String::with_capacity(raw.len());
        let mut in_tag = false;
        for c in raw.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => text.push(c),
                _ => {}
            }
        }
        // Filter out debug/meta lines (llama.cpp log lines starting with common prefixes)
        let text: String = text.lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty()
                    && !line.starts_with('#')
                    && !line.starts_with("llama_")
                    && !line.starts_with("main:")
                    && !line.starts_with("Log ")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text = text.trim().to_string();
        if text.is_empty() { return Err("识别结果为空".into()); }
        Ok(text)
    }
}
