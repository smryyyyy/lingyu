use arboard::Clipboard;
use std::thread;
use std::time::Duration;

/// Copy text to clipboard with retry logic.
/// Windows clipboard can be locked by other processes (screenshot tools, IDEs, etc.).
/// We retry up to 5 times with 100ms delays before giving up.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    for attempt in 0..5 {
        match Clipboard::new() {
            Ok(mut clipboard) => {
                return clipboard.set_text(text).map_err(|e| format!("复制到剪贴板失败：{e}"));
            }
            Err(e) if attempt < 4 => {
                thread::sleep(Duration::from_millis(100 * (attempt as u64 + 1)));
            }
            Err(e) => {
                return Err(format!("打开剪贴板失败：{e}"));
            }
        }
    }
    unreachable!()
}
