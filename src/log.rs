use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

pub fn init(debug: bool) {
    if !debug { return; }
    ENABLED.store(true, Ordering::Relaxed);
    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("lingyu-debug.log");
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
        *LOG_FILE.lock().unwrap() = Some(file);
        eprintln!("调试日志：{}", path.display());
    }
}

pub fn debug(msg: &str) {
    if !ENABLED.load(Ordering::Relaxed) { return; }
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut f) = *guard {
            let now = chrono::Local::now().format("%H:%M:%S%.3f");
            let _ = writeln!(f, "[{now}] {msg}");
            let _ = f.flush();
        }
    }
}

#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => { $crate::log::debug(&format!($($arg)*)) };
}
