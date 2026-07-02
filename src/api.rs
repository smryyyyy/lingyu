use reqwest::multipart;
use std::time::Duration;

pub async fn transcribe(base_url: &str, api_key: &str, model: &str, wav_data: Vec<u8>) -> Result<String, String> {
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("无效的 API URL".into());
    }

    let url = format!("{}/audio/transcriptions", base_url.trim_end_matches('/'));
    let file_part = multipart::Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("表单错误：{e}"))?;
    let form = multipart::Form::new()
        .text("model", model.to_string())
        .text("response_format", "json")
        .part("file", file_part);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120)).connect_timeout(Duration::from_secs(10))
        .build().map_err(|e| format!("HTTP 客户端错误：{e}"))?;

    let resp = client.post(&url).bearer_auth(api_key).multipart(form)
        .send().await.map_err(|e| format!("请求失败：{e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API 错误 {status}：{body}"));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| format!("JSON 解析错误：{e}"))?;
    json["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("响应中没有 'text' 字段：{json}"))
}

/// Blocking version — runs on a background thread, no async runtime needed.
pub fn transcribe_blocking(base_url: &str, api_key: &str, model: &str, wav_data: Vec<u8>) -> Result<String, String> {
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("无效的 API URL".into());
    }

    let url = format!("{}/audio/transcriptions", base_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .build().map_err(|e| format!("HTTP 客户端错误：{e}"))?;

    let file_part = reqwest::blocking::multipart::Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("表单错误：{e}"))?;
    let form = reqwest::blocking::multipart::Form::new()
        .text("model", model.to_string())
        .text("response_format", "json")
        .part("file", file_part);

    let resp = client.post(&url).bearer_auth(api_key).multipart(form)
        .send().map_err(|e| format!("请求失败：{e}"))?;

    if !resp.status().is_success() {
        return Err(format!("API 错误 {}：{}", resp.status(), resp.text().unwrap_or_default()));
    }

    let json: serde_json::Value = resp.json().map_err(|e| format!("JSON 解析错误：{e}"))?;
    json["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("响应中没有 'text' 字段：{json}"))
}
