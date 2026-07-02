use std::path::PathBuf;
use std::path::Path;

/// Active transcription backend.
#[derive(Clone, Copy, PartialEq)]
pub enum TranscriptionService { Api, Local }

/// Application configuration loaded from environment and `.env` file.
pub struct Config {
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub api_model: String,
    pub db_path: PathBuf,
    pub bin_dir: PathBuf,
    pub models_dir: PathBuf,
    pub always_on_top: bool,
    pub record_shortcut: String,
}

impl Config {
    pub fn load() -> Self {
        let _ = dotenvy::dotenv();

        let api_base_url = std::env::var("API_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:1234/v1".into());
        let api_key = std::env::var("API_KEY").ok();
        let api_model = std::env::var("API_MODEL")
            .unwrap_or_else(|_| "whisper-1".into());

        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lingyu");
        std::fs::create_dir_all(&data_dir).ok();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700));
        }

        let db_path = data_dir.join("history.db");
        let bin_dir = data_dir.join("bin");
        let models_dir = data_dir.join("models");
        std::fs::create_dir_all(&bin_dir).ok();
        std::fs::create_dir_all(&models_dir).ok();

        let always_on_top = std::env::var("ALWAYS_ON_TOP")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(true);

        let record_shortcut = std::env::var("RECORD_SHORTCUT")
            .unwrap_or_else(|_| "F6".into());

        Self { api_base_url, api_key, api_model, db_path, bin_dir, models_dir, always_on_top, record_shortcut }
    }
}

// ── SenseVoice local model presets ────────────────────────────────

pub struct LocalModelPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub file_name: &'static str,
    pub size_label: &'static str,
    pub url: &'static str,
}

pub const LOCAL_MODEL_PRESETS: &[LocalModelPreset] = &[
    LocalModelPreset {
        id: "sensevoice-q8",
        label: "SenseVoice Q8",
        file_name: "sensevoice-small-q8.gguf",
        size_label: "~242 MB",
        url: "https://huggingface.co/FunAudioLLM/SenseVoiceSmall-GGUF/resolve/main/sensevoice-small-q8.gguf",
    },
    LocalModelPreset {
        id: "sensevoice-f16",
        label: "SenseVoice F16",
        file_name: "sensevoice-small-f16.gguf",
        size_label: "~470 MB",
        url: "https://huggingface.co/FunAudioLLM/SenseVoiceSmall-GGUF/resolve/main/sensevoice-small-f16.gguf",
    },
];

pub const DEFAULT_LOCAL_MODEL: &str = "sensevoice-q8";

pub fn find_local_model(id: &str) -> Option<&'static LocalModelPreset> {
    LOCAL_MODEL_PRESETS.iter().find(|m| m.id == id)
}

// ── FunASR binary info (per platform) ─────────────────────────────

pub struct FunasrBinary {
    pub binary_name: &'static str,
    pub url: &'static str,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const FUNASR_BINARY: FunasrBinary = FunasrBinary {
    binary_name: "llama-funasr-sensevoice",
    url: "https://github.com/modelscope/FunASR/releases/download/runtime-llamacpp-v0.1.4/funasr-llamacpp-macos-arm64.tar.gz",
};

#[cfg(target_os = "windows")]
pub const FUNASR_BINARY: FunasrBinary = FunasrBinary {
    binary_name: "llama-funasr-sensevoice.exe",
    url: "https://github.com/modelscope/FunASR/releases/download/runtime-llamacpp-v0.1.4/funasr-llamacpp-windows-x64.zip",
};

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub const FUNASR_BINARY: FunasrBinary = FunasrBinary {
    binary_name: "llama-funasr-sensevoice",
    url: "https://github.com/modelscope/FunASR/releases/download/runtime-llamacpp-v0.1.4/funasr-llamacpp-linux-arm64.tar.gz",
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub const FUNASR_BINARY: FunasrBinary = FunasrBinary {
    binary_name: "llama-funasr-sensevoice",
    url: "https://github.com/modelscope/FunASR/releases/download/runtime-llamacpp-v0.1.4/funasr-llamacpp-linux-x64.tar.gz",
};

// ── VAD model ─────────────────────────────────────────────────────
pub const VAD_MODEL_URL: &str =
    "https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf";
pub const VAD_MODEL_FILENAME: &str = "fsmn-vad.gguf";

/// Check if FunASR binary is ready at the given directory.
pub fn funasr_binary_ready(bin_dir: &Path) -> bool {
    bin_dir.join(FUNASR_BINARY.binary_name).exists()
}

/// Check if VAD model is ready.
pub fn vad_ready(models_dir: &Path) -> bool {
    models_dir.join(VAD_MODEL_FILENAME).exists()
}
