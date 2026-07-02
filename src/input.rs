use arboard::Clipboard;

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("打开剪贴板失败：{e}"))?;
    clipboard.set_text(text).map_err(|e| format!("复制到剪贴板失败：{e}"))?;
    Ok(())
}
