use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::audio::Recorder;
use crate::config::{self, Config, TranscriptionService};
use crate::db::Db;
use crate::input;
use crate::local_stt::LocalWhisper;

// ── CSS ──────────────────────────────────────────────────────────────────────

const CSS: &str = r#"
    window.main-window {
        background-color: transparent;
    }
    window.main-window.macos-bg {
        background-color: rgba(17, 17, 17, 0.92);
    }
    .macos-bg .mic-btn {
        min-width: 68px;
        min-height: 68px;
    }
    .mic-btn {
        min-width: 72px;
        min-height: 72px;
        border-radius: 9999px;
        background-image: none;
        background-color: #dc2626;
        color: white;
        font-size: 32px;
        font-weight: 600;
        border: none;
        box-shadow: none;
        outline: none;
        -gtk-icon-shadow: none;
        -gtk-icon-size: 32px;
        padding: 0;
    }
    .mic-btn:hover {
        background-image: none;
        background-color: #b91c1c;
        box-shadow: none;
    }
    .mic-btn:active {
        background-image: none;
        background-color: #991b1b;
        box-shadow: none;
    }
    .mic-btn.recording,
    .mic-btn.recording:hover {
        background-image: none;
        background-color: #16a34a;
        box-shadow: none;
        animation: pulse 1s ease-in-out infinite;
    }
    .mic-btn.processing,
    .mic-btn.processing:hover {
        background-image: none;
        background-color: #d97706;
        box-shadow: none;
    }
    .mic-btn.done,
    .mic-btn.done:hover {
        background-image: none;
        background-color: #16a34a;
        box-shadow: none;
    }
    @keyframes pulse {
        0%   { opacity: 1.0; }
        50%  { opacity: 0.7; }
        100% { opacity: 1.0; }
    }
    .brand-label {
        color: rgba(255, 255, 255, 0.4);
        font-size: 9px;
        font-weight: 500;
        letter-spacing: 1px;
        margin-top: 4px;
    }
    .status-label {
        color: #e2e8f0;
        font-size: 10px;
        font-weight: 500;
        background-color: rgba(15, 23, 42, 0.75);
        border-radius: 6px;
        padding: 3px 8px;
    }
"#;

// ── SVG mic icon ─────────────────────────────────────────────────────────────

const MIC_SVG: &[u8] = include_bytes!("../icons/microphone.svg");
const MIC_REC_SVG: &[u8] = include_bytes!("../icons/microphone_recording.svg");

fn load_mic_pixbuf(svg_data: &[u8]) -> gdk_pixbuf::Pixbuf {
    let loader = gdk_pixbuf::PixbufLoader::with_type("svg")
        .expect("SVG loader");
    loader.write(svg_data).expect("write SVG");
    loader.close().expect("close SVG loader");
    loader.pixbuf().expect("SVG pixbuf")
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn show_status(label: &gtk4::Label, text: &str) {
    label.set_text(text);
    label.set_opacity(1.0);
}

fn hide_status(label: &gtk4::Label) {
    label.set_opacity(0.0);
}

// ── State ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Idle,
    Recording,
    Processing,
}

struct RuntimeState {
    active_service: TranscriptionService,
    active_provider: String,
    api_base_url: String,
    api_key: Option<String>,
    api_model: String,
    local_whisper: Option<LocalWhisper>,
    downloading: bool,
}

// ── build_ui ─────────────────────────────────────────────────────────────────

pub fn build_ui(app: &gtk4::Application, config: Arc<Config>) {
    // ── CSS ──
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS);
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("no display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // ── DB ──
    let db = match Db::open(&config.db_path) {
        Ok(d) => Arc::new(Mutex::new(Some(d))),
        Err(_) => Arc::new(Mutex::new(None)),
    };

    // ── Load saved settings ──
    let initial_service = {
        let d = db.lock().unwrap();
        match d.as_ref().and_then(|d| d.get_setting("transcription_mode").ok().flatten()) {
            Some(ref v) if v == "local" || config::find_local_model(v).is_some() => TranscriptionService::Local,
            _ => TranscriptionService::Api,
        }
    };
    let initial_provider = {
        let d = db.lock().unwrap();
        d.as_ref()
            .and_then(|d| d.get_setting("transcription_mode").ok().flatten())
            .unwrap_or_else(|| "custom".to_string())
    };
    let initial_api_url = {
        let d = db.lock().unwrap();
        d.as_ref()
            .and_then(|d| d.get_setting("api_custom_url").ok().flatten())
            .unwrap_or_else(|| config.api_base_url.clone())
    };
    let initial_api_key = {
        let d = db.lock().unwrap();
        d.as_ref()
            .and_then(|d| d.get_setting("api_key_custom").ok().flatten())
            .or_else(|| config.api_key.clone())
    };
    let initial_api_model = {
        let d = db.lock().unwrap();
        d.as_ref()
            .and_then(|d| d.get_setting("api_custom_model").ok().flatten())
            .unwrap_or_else(|| config.api_model.clone())
    };

    // ── Runtime state ──
    let runtime = Rc::new(RefCell::new(RuntimeState {
        active_service: initial_service,
        active_provider: initial_provider.clone(),
        api_base_url: initial_api_url,
        api_key: initial_api_key,
        api_model: initial_api_model,
        local_whisper: None,
        downloading: false,
    }));

    // Restore local model at startup if it was selected and exists on disk
    if initial_service == TranscriptionService::Local {
        if let Some(preset) = config::find_local_model(&initial_provider) {
            let model_path = config.models_dir.join(preset.file_name);
            if model_path.exists() && config::funasr_binary_ready(&config.bin_dir) {
                if let Ok(whisper) = LocalWhisper::new(&config.bin_dir, &model_path, &config.models_dir) {
                    runtime.borrow_mut().local_whisper = Some(whisper);
                }
            }
        }
    }

    let state = Rc::new(RefCell::new(State::Idle));
    let recorder = Rc::new(RefCell::new(Recorder::new()));

    // ── Window ──
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("灵语")
        .default_width(88)
        .default_height(100)
        .decorated(false)
        .resizable(false)
        .css_classes(vec!["main-window"])
        .build();

    #[cfg(target_os = "macos")]
    window.add_css_class("macos-bg");

    // ── Layout ──
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_halign(gtk4::Align::Center);
    vbox.set_valign(gtk4::Align::Center);

    // ── Mic icon ──
    let mic_pixbuf = load_mic_pixbuf(MIC_SVG);
    let mic_rec_pixbuf = load_mic_pixbuf(MIC_REC_SVG);
    let icon = gtk4::Image::from_pixbuf(Some(&mic_pixbuf));
    icon.set_pixel_size(32);

    let button = gtk4::Button::new();
    button.set_child(Some(&icon));
    button.add_css_class("mic-btn");
    button.set_size_request(72, 72);
    button.set_halign(gtk4::Align::Center);
    button.set_focusable(false);

    let status = gtk4::Label::new(Some(" "));
    status.add_css_class("status-label");
    status.set_opacity(0.0);

    vbox.append(&button);

    // macOS: brand label + overlay
    #[cfg(target_os = "macos")]
    {
        icon.set_pixel_size(36);
        button.set_size_request(68, 68);
        let brand = gtk4::Label::new(Some("灵语"));
        brand.set_justify(gtk4::Justification::Center);
        brand.add_css_class("brand-label");
        vbox.append(&brand);
        window.set_default_size(96, 110);
    }

    // Status: overlay on macOS, normal on others
    #[cfg(not(target_os = "macos"))]
    vbox.append(&status);

    let handle = gtk4::WindowHandle::new();

    #[cfg(target_os = "macos")]
    {
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&vbox));
        status.set_halign(gtk4::Align::Center);
        status.set_valign(gtk4::Align::End);
        status.set_margin_bottom(4);
        overlay.add_overlay(&status);
        handle.set_child(Some(&overlay));
    }

    #[cfg(not(target_os = "macos"))]
    handle.set_child(Some(&vbox));

    window.set_child(Some(&handle));

    // ── Left-click handler ──
    let state_l = Rc::clone(&state);
    let recorder_l = Rc::clone(&recorder);
    let runtime_l = Rc::clone(&runtime);
    let icon_l = icon.clone();
    let mic_pixbuf = mic_pixbuf.clone();
    let mic_rec_pixbuf = mic_rec_pixbuf.clone();
    let db_l = Arc::clone(&db);
    let status_l = status.clone();

    button.connect_clicked(move |_| {
        let current = *state_l.borrow();
        match current {
            State::Idle | State::Processing => {
                if let Err(e) = recorder_l.borrow_mut().start() {
                    log_error(&format!("录音失败：{e}"));
                    show_status(&status_l, "错误，看日志");
                    return;
                }
                *state_l.borrow_mut() = State::Recording;
                icon_l.set_from_pixbuf(Some(&mic_rec_pixbuf));
                show_status(&status_l, "录音中...");
            }
            State::Recording => {
                let wav_data = match recorder_l.borrow_mut().stop() {
                    Ok(d) => d,
                    Err(e) => {
                        *state_l.borrow_mut() = State::Idle;
                        icon_l.set_from_pixbuf(Some(&mic_pixbuf));
                        hide_status(&status_l);
                        show_status(&status_l, &format!("{e}"));
                        return;
                    }
                };
                *state_l.borrow_mut() = State::Processing;
                icon_l.set_from_pixbuf(Some(&mic_pixbuf));
                show_status(&status_l, "识别中...");

                let state_p = Rc::clone(&state_l);
                let icon_p = icon_l.clone();
                let mic_pixbuf = mic_pixbuf.clone();
                let runtime_p = Rc::clone(&runtime_l);
                let db_p = Arc::clone(&db_l);
                let status_p = status_l.clone();
                let sample_rate = recorder_l.borrow().sample_rate();

                // Extract data BEFORE spawning thread (Rc<RefCell> is !Send)
                let (mode_is_api, api_key_s, api_url_s, api_model_s, local_whisper_s) = {
                    let rt = runtime_p.borrow();
                    let is_api = matches!(rt.active_service, TranscriptionService::Api);
                    let key = rt.api_key.clone().unwrap_or_default();
                    let url = rt.api_base_url.clone();
                    let mdl = rt.api_model.clone();
                    let local = rt.local_whisper.clone();
                    (is_api, key, url, mdl, local)
                };

                // Spawn background thread (no tokio needed)
                let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
                std::thread::spawn(move || {
                    let result = if mode_is_api {
                        crate::api::transcribe_blocking(&api_url_s, &api_key_s, &api_model_s, wav_data)
                    } else if let Some(l) = local_whisper_s {
                        l.transcribe(&wav_data, sample_rate)
                    } else {
                        Err("本地引擎未就绪".into())
                    };
                    let _ = tx.send(result);
                });

                // Poll channel to get result and update UI
                glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                    match rx.try_recv() {
                        Ok(Ok(text)) => {
                            if let Err(e) = input::copy_to_clipboard(&text) {
                                log_error(&format!("复制到剪贴板失败：{e}"));
                                show_status(&status_p, "错误，看日志");
                            } else {
                                show_status(&status_p, "已复制");
                                if let Ok(d) = db_p.lock() {
                                    if let Some(ref d) = *d {
                                        let _ = d.insert(&text);
                                    }
                                }
                            }
                            let st = status_p.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_secs(2),
                                move || hide_status(&st),
                            );
                        }
                        Ok(Err(e)) => {
                            log_error(&format!("识别失败：{e}"));
                            show_status(&status_p, "错误，看日志");
                            let st = status_p.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_secs(4),
                                move || hide_status(&st),
                            );
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            return glib::ControlFlow::Continue;
                        }
                        Err(_) => {}
                    }
                    if *state_p.borrow() == State::Processing {
                        *state_p.borrow_mut() = State::Idle;
                        icon_p.set_from_pixbuf(Some(&mic_pixbuf));
                    }
                    glib::ControlFlow::Break
                });
            }
        }
    });

    // ── Right-click menu ──
    let stt_section = gtk4::gio::Menu::new();
    stt_section.append(Some("自定义 API"), Some("app.transcription-mode::custom"));

    let stt_local_section = gtk4::gio::Menu::new();
    for lm in config::LOCAL_MODEL_PRESETS {
        stt_local_section.append(
            Some(&format!("{} ({})", lm.label, lm.size_label)),
            Some(&format!("app.transcription-mode::{}", lm.id)),
        );
    }

    let settings_section = gtk4::gio::Menu::new();
    settings_section.append(Some("窗口置顶"), Some("app.always-on-top"));
    settings_section.append(Some("设置快捷键..."), Some("app.set-shortcut"));
    settings_section.append(Some("模型文件夹"), Some("app.open-model-folder"));

    let actions_section = gtk4::gio::Menu::new();
    actions_section.append(Some("历史记录"), Some("app.show-history"));
    actions_section.append(Some("关于灵语"), Some("app.about"));
    actions_section.append(Some("退出"), Some("app.quit"));

    let menu = gtk4::gio::Menu::new();
    menu.append_section(Some("语音转文字 — API"), &stt_section);
    menu.append_section(Some("语音转文字 — 本地"), &stt_local_section);
    menu.append_section(Some("设置"), &settings_section);
    menu.append_section(None, &actions_section);

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&button);
    popover.set_has_arrow(true);

    let pop = popover.clone();
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_pressed(move |g, _, _, _| {
        g.set_state(gtk4::EventSequenceState::Claimed);
        pop.popup();
    });
    button.add_controller(gesture);

    // ── Action: transcription mode ──
    let runtime_m = Rc::clone(&runtime);
    let db_m = Arc::clone(&db);
    let status_m = status.clone();
    let state_m = Rc::clone(&state);
    let recorder_m = Rc::clone(&recorder);
    let config_m = Arc::clone(&config);
    let window_m = window.clone();

    let mode_action = gtk4::gio::SimpleAction::new_stateful(
        "transcription-mode",
        Some(&String::static_variant_type()),
        &initial_provider.to_variant(),
    );
    mode_action.connect_activate(move |action, param| {
        let Some(id) = param.and_then(|p| p.get::<String>()) else { return };
        let id = id.to_string();

        if *state_m.borrow() == State::Recording {
            let _ = recorder_m.borrow_mut().stop();
            *state_m.borrow_mut() = State::Idle;
            hide_status(&status_m);
        }

        let db_guard = db_m.lock().unwrap();

        if let Some(preset) = config::find_local_model(&id) {
            // Set radio indicator immediately so user sees feedback
            action.set_state(&id.to_variant());
            {
                let mut rt = runtime_m.borrow_mut();
                rt.active_service = TranscriptionService::Local;
                rt.active_provider = id.clone();
            }
            if let Some(ref d) = *db_guard {
                let _ = d.set_setting("transcription_mode", &id);
            }

            let bin_dir = config_m.bin_dir.clone();
            let model_path = config_m.models_dir.join(preset.file_name);

            if !config::funasr_binary_ready(&bin_dir) {
                log_error("FunASR 引擎未安装，请在设置中检查模型文件夹");
                show_status(&status_m, "错误，看日志");
                return;
            }
            if model_path.exists() {
                match LocalWhisper::new(&bin_dir, &model_path, &config_m.models_dir) {
                    Ok(whisper) => {
                        runtime_m.borrow_mut().local_whisper = Some(whisper);
                    }
                    Err(e) => {
                        log_error(&format!("本地模型加载失败：{e}"));
                        show_status(&status_m, "错误，看日志");
                    }
                }
            } else {
                // Model not downloaded yet — start download with progress
                let url = preset.url.to_string();
                let model_path_dl = model_path.clone();
                let status_dl = status_m.clone();
                let config_dl = Arc::clone(&config_m);
                let db_dl = Arc::clone(&db_m);
                let provider_id = preset.id.to_string();
                show_status(&status_m, "正在下载模型...");

                enum DlMsg { Progress(u64, Option<u64>), Done, Error(String) }
                let (tx, rx) = std::sync::mpsc::channel::<DlMsg>();
                std::thread::spawn(move || {
                    let part_path = model_path_dl.with_extension("gguf.part");
                    let result = (|| -> Result<(), String> {
                        let resp = reqwest::blocking::Client::new()
                            .get(&url)
                            .send()
                            .map_err(|e| format!("下载请求失败：{e}"))?;
                        if !resp.status().is_success() {
                            return Err(format!("HTTP {}", resp.status()));
                        }
                        let total = resp.content_length();
                        use std::io::{Read, Write};
                        let mut file = std::fs::File::create(&part_path)
                            .map_err(|e| format!("创建文件失败：{e}"))?;
                        let mut reader = resp;
                        let mut buf = [0u8; 65536];
                        let mut downloaded: u64 = 0;
                        loop {
                            let n = reader.read(&mut buf)
                                .map_err(|e| format!("读取错误：{e}"))?;
                            if n == 0 { break; }
                            file.write_all(&buf[..n])
                                .map_err(|e| format!("写入错误：{e}"))?;
                            downloaded += n as u64;
                            let _ = tx.send(DlMsg::Progress(downloaded, total));
                        }
                        drop(file);
                        std::fs::rename(&part_path, &model_path_dl)
                            .map_err(|e| format!("重命名失败：{e}"))?;
                        Ok(())
                    })();
                    match result {
                        Ok(()) => { let _ = tx.send(DlMsg::Done); }
                        Err(e) => { let _ = tx.send(DlMsg::Error(e)); }
                    }
                });
                glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                    let mut last = None;
                    while let Ok(msg) = rx.try_recv() { last = Some(msg); }
                    match last {
                        Some(DlMsg::Progress(dl, total)) => {
                            let dl_mb = dl as f64 / (1024.0 * 1024.0);
                            let text = if let Some(t) = total {
                                let total_mb = t as f64 / (1024.0 * 1024.0);
                                format!("正在下载：{dl_mb:.0} / {total_mb:.0} MB")
                            } else {
                                format!("正在下载：{dl_mb:.0} MB")
                            };
                            show_status(&status_dl, &text);
                            glib::ControlFlow::Continue
                        }
                        Some(DlMsg::Done) => {
                            let model_path = config_dl.models_dir.join(
                                config::find_local_model(&provider_id).map(|m| m.file_name).unwrap_or("")
                            );
                            if let Ok(d) = db_dl.lock() {
                                if let Some(ref d) = *d {
                                    let _ = d.set_setting("transcription_mode", &provider_id);
                                }
                            }
                            if let Ok(whisper) = LocalWhisper::new(
                                &config_dl.bin_dir,
                                &model_path,
                                &config_dl.models_dir,
                            ) {
                                show_status(&status_dl, "模型准备就绪 ✓");
                                let st = status_dl.clone();
                                glib::timeout_add_local_once(
                                    std::time::Duration::from_secs(2),
                                    move || hide_status(&st),
                                );
                            }
                            glib::ControlFlow::Break
                        }
                        Some(DlMsg::Error(e)) => {
                            log_error(&format!("模型下载失败：{e}"));
                            show_status(&status_dl, "模型下载失败");
                            let st = status_dl.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_secs(3),
                                move || hide_status(&st),
                            );
                            glib::ControlFlow::Break
                        }
                        None => glib::ControlFlow::Continue,
                    }
                });
            }
            return;
        } else if id == "custom" {
            drop(db_guard);
            show_custom_api_dialog(&window_m, &runtime_m, &db_m, action, &status_m, &config_m);
            return; // dialog handles its own status
        }

        let st = status_m.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || hide_status(&st));
    });
    app.add_action(&mode_action);

    // ── Always on top ──
    let aat_action = gtk4::gio::SimpleAction::new_stateful("always-on-top", None, &config.always_on_top.to_variant());
    let win_aat = window.clone();
    aat_action.connect_activate(move |action, _| {
        let new_state = !action.state().and_then(|s| s.get::<bool>()).unwrap_or(true);
        action.change_state(&new_state.to_variant());
        if new_state { macos_set_window_floating(&win_aat); }
        else { macos_unset_window_floating(&win_aat); }
    });
    app.add_action(&aat_action);

    // ── Set shortcut ──
    let win_sc = window.clone();
    let app_sc = app.clone();
    let status_sc = status.clone();
    let db_sc = Arc::clone(&db);
    let sc_action = gtk4::gio::SimpleAction::new("set-shortcut", None);
    sc_action.connect_activate(move |_, _| {
        let dialog = gtk4::Dialog::with_buttons(
            Some("设置快捷键"),
            Some(&win_sc),
            gtk4::DialogFlags::MODAL,
            &[("确定", gtk4::ResponseType::Accept), ("取消", gtk4::ResponseType::Reject)],
        );
        dialog.set_default_size(220, 100);
        let content = dialog.content_area();
        content.set_margin_start(12); content.set_margin_end(12);
        content.set_margin_top(12); content.set_margin_bottom(12);
        content.set_spacing(8);

        let label = gtk4::Label::new(Some("选择快捷键："));
        content.append(&label);

        let combo = gtk4::DropDown::from_strings(&[
            "F1", "F2", "F3", "F4", "F5", "F6",
            "F7", "F8", "F9", "F10", "F11", "F12",
        ]);
        combo.set_selected(5);
        content.append(&combo);

        let app_dialog = app_sc.clone();
        let db_s = Arc::clone(&db_sc);
        let st = status_sc.clone();

        dialog.connect_response(move |d, resp| {
            if resp == gtk4::ResponseType::Accept {
                let idx = combo.selected().to_string();
                let shortcut = match idx.as_str() {
                    "0" => "F1", "1" => "F2", "2" => "F3", "3" => "F4",
                    "4" => "F5", "5" => "F6", "6" => "F7", "7" => "F8",
                    "8" => "F9", "9" => "F10", "10" => "F11", "11" => "F12",
                    _ => "F6",
                };
                if let Ok(dg) = db_s.lock() {
                    if let Some(ref dg) = *dg {
                        let _ = dg.set_setting("record_shortcut", shortcut);
                    }
                }
                app_dialog.set_accels_for_action("app.record", &[shortcut]);
                show_status(&st, &format!("快捷键：{shortcut}"));
                let st2 = st.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || hide_status(&st2));
            }
            d.close();
        });
        dialog.show();
    });
    app.add_action(&sc_action);

    // ── Open model folder ──
    let models_dir_open = config.models_dir.clone();
    let open_model_action = gtk4::gio::SimpleAction::new("open-model-folder", None);
    open_model_action.connect_activate(move |_, _| {
        let path_str = models_dir_open.to_string_lossy().to_string();
        std::fs::create_dir_all(&*models_dir_open).ok();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&path_str).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(&path_str).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&path_str).spawn();
    });
    app.add_action(&open_model_action);

    // ── History ──
    let db_hist = Arc::clone(&db);
    let win_hist = window.clone();
    let hist_action = gtk4::gio::SimpleAction::new("show-history", None);
    hist_action.connect_activate(move |_, _| {
        show_history_dialog(&win_hist, &db_hist);
    });
    app.add_action(&hist_action);

    // ── About ──
    let win_about = window.clone();
    let about_action = gtk4::gio::SimpleAction::new("about", None);
    about_action.connect_activate(move |_, _| {
        let about = gtk4::AboutDialog::new();
        about.set_program_name(Some("灵语"));
        about.set_version(Some("0.1.0"));
        about.set_comments(Some("浮窗语音转文字\n本地 SenseVoice + API 模式"));
        about.set_license_type(gtk4::License::MitX11);
        about.set_transient_for(Some(&win_about));
        about.present();
    });
    app.add_action(&about_action);

    // ── Quit ──
    let app_q = app.clone();
    let quit_action = gtk4::gio::SimpleAction::new("quit", None);
    quit_action.connect_activate(move |_, _| { app_q.quit(); });
    app.add_action(&quit_action);

    // ── Register keyboard shortcut ──
    let sc_key = db.lock().ok()
        .and_then(|d| d.as_ref().and_then(|d| d.get_setting("record_shortcut").ok().flatten()))
        .unwrap_or_else(|| "F6".to_string());
    // Register record action with shortcut
    let record_action = gtk4::gio::SimpleAction::new("record", None);
    let btn_rec = button.clone();
    let state_rec = Rc::clone(&state);
    record_action.connect_activate(move |_, _| {
        if *state_rec.borrow() == State::Idle {
            btn_rec.emit_clicked();
        }
    });
    app.add_action(&record_action);
    app.set_accels_for_action("app.record", &[&sc_key]);

    // ── Window position + present ──
    position_window(&window, &db);
    window.present();

    // ── First-run: auto-download dependencies ──
    let bin_dir = config.bin_dir.clone();
    let models_dir = config.models_dir.clone();
    let status_dl = status.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
        let bin_path = bin_dir.join(config::FUNASR_BINARY.binary_name);
        let vad_path = models_dir.join(config::VAD_MODEL_FILENAME);
        if bin_path.exists() && vad_path.exists() { return; }

        show_status(&status_dl, "首次启动：检查依赖...");
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        std::thread::spawn(move || {
            if !bin_path.exists() {
                tx.send("正在下载 FunASR 引擎...".into()).ok();
                let _ = download_funasr_binary(&bin_dir).map(|_| tx.send("FunASR 引擎就绪 ✓".into()));
            }
            if !vad_path.exists() {
                tx.send("正在下载 VAD 模型...".into()).ok();
                let _ = download_file(config::VAD_MODEL_URL, &vad_path)
                    .map(|_| tx.send("所有依赖就绪 ✓".into()));
            }
        });

        let status_poll = status_dl.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(msg) = rx.try_recv() {
                show_status(&status_poll, &msg);
            }
            glib::ControlFlow::Continue
        });
    });

    // Default: always-on-top
    macos_set_window_floating(&window);
}

// ── Error logging helper ──────────────────────────────────────────────────────

/// Write an error message to ~/lingyu-debug.log and print to stderr.
fn log_error(msg: &str) {
    eprintln!("灵语错误：{msg}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true).append(true).open(
            dirs::home_dir().unwrap_or_default().join("lingyu-debug.log"),
        )
    {
        let _ = std::io::Write::write_all(
            &mut f,
            format!(
                "[{}] {msg}\n",
                chrono::Local::now().format("%H:%M:%S%.3f")
            )
            .as_bytes(),
        );
    }
}

// ── Dialogs ──────────────────────────────────────────────────────────────────

fn show_history_dialog(window: &gtk4::ApplicationWindow, db: &Arc<Mutex<Option<Db>>>) {
    let entries = if let Ok(d) = db.lock() {
        d.as_ref().and_then(|d| d.recent(50).ok()).unwrap_or_default()
    } else { vec![] };

    let dialog = gtk4::Dialog::builder()
        .title("历史记录")
        .transient_for(window)
        .modal(true)
        .default_width(400)
        .default_height(300)
        .build();

    let scrolled = gtk4::ScrolledWindow::new();
    let text = gtk4::TextBuffer::new(None);
    let text_view = gtk4::TextView::with_buffer(&text);
    text_view.set_editable(false);
    text_view.set_wrap_mode(gtk4::WrapMode::Word);

    for entry in &entries {
        text.insert_at_cursor(&format!("[{}] {}\n\n", entry.created_at, entry.text));
    }

    scrolled.set_child(Some(&text_view));
    dialog.content_area().append(&scrolled);
    dialog.show();
}

// ── Custom API dialog ─────────────────────────────────────────────────────────

fn show_custom_api_dialog(
    parent: &gtk4::ApplicationWindow,
    runtime: &Rc<RefCell<RuntimeState>>,
    db: &Arc<Mutex<Option<Db>>>,
    action: &gtk4::gio::SimpleAction,
    status: &gtk4::Label,
    config: &Arc<Config>,
) {
    let previous_provider = runtime.borrow().active_provider.clone();

    let dialog = gtk4::Window::builder()
        .title("自定义 API 配置")
        .default_width(400)
        .default_height(220)
        .transient_for(parent)
        .modal(true)
        .build();

    let grid = gtk4::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    // Base URL
    let url_label = gtk4::Label::new(Some("基础 URL"));
    url_label.set_halign(gtk4::Align::End);
    let url_entry = gtk4::Entry::new();
    url_entry.set_hexpand(true);
    url_entry.set_placeholder_text(Some("https://api.example.com/v1"));
    grid.attach(&url_label, 0, 0, 1, 1);
    grid.attach(&url_entry, 1, 0, 2, 1);

    // API Key
    let key_label = gtk4::Label::new(Some("API 密钥"));
    key_label.set_halign(gtk4::Align::End);
    let key_entry = gtk4::Entry::new();
    key_entry.set_hexpand(true);
    key_entry.set_placeholder_text(Some("（可选）"));
    key_entry.set_input_purpose(gtk4::InputPurpose::Password);
    key_entry.set_visibility(false);
    grid.attach(&key_label, 0, 1, 1, 1);
    grid.attach(&key_entry, 1, 1, 2, 1);

    // Model
    let model_label = gtk4::Label::new(Some("模型"));
    model_label.set_halign(gtk4::Align::End);
    let model_entry = gtk4::Entry::new();
    model_entry.set_hexpand(true);
    model_entry.set_placeholder_text(Some("whisper-1"));
    grid.attach(&model_label, 0, 2, 1, 1);
    grid.attach(&model_entry, 1, 2, 2, 1);

    // Pre-populate from DB
    if let Ok(d) = db.lock() {
        if let Some(ref d) = *d {
            if let Ok(Some(url)) = d.get_setting("api_custom_url") {
                url_entry.set_text(&url);
            }
            if let Ok(Some(key)) = d.get_setting("api_custom_key") {
                key_entry.set_text(&key);
            }
            if let Ok(Some(model)) = d.get_setting("api_custom_model") {
                model_entry.set_text(&model);
            }
        }
    }

    // Buttons
    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::End);
    let cancel_btn = gtk4::Button::with_label("取消");
    let save_btn = gtk4::Button::with_label("保存");
    btn_box.append(&cancel_btn);
    btn_box.append(&save_btn);
    grid.attach(&btn_box, 0, 3, 3, 1);

    dialog.set_child(Some(&grid));

    // Cancel → revert radio to previous provider
    let action_cancel = action.clone();
    let prev = previous_provider.clone();
    let dialog_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        action_cancel.set_state(&prev.to_variant());
        dialog_cancel.close();
    });

    // Save → persist + switch
    let runtime_save = Rc::clone(runtime);
    let db_save = Arc::clone(db);
    let config_save = Arc::clone(config);
    let action_save = action.clone();
    let status_save = status.clone();
    let dialog_save = dialog.clone();
    save_btn.connect_clicked(move |_| {
        let url = url_entry.text().to_string();
        let key_text = key_entry.text().to_string();
        let model = model_entry.text().to_string();

        if url.is_empty() || model.is_empty() {
            return;
        }

        let api_key = if key_text.is_empty() {
            None
        } else {
            Some(key_text.clone())
        };

        // Persist to DB
        if let Ok(d) = db_save.lock() {
            if let Some(ref d) = *d {
                let _ = d.set_setting("api_custom_url", &url);
                if let Some(ref k) = api_key {
                    let _ = d.set_setting("api_custom_key", k);
                }
                let _ = d.set_setting("api_custom_model", &model);
                let _ = d.set_setting("transcription_mode", "custom");
            }
        }

        // Update RuntimeState
        {
            let mut rt = runtime_save.borrow_mut();
            rt.active_service = TranscriptionService::Api;
            rt.active_provider = "custom".to_string();
            rt.api_base_url = url;
            rt.api_key = api_key;
            rt.api_model = model;
            rt.local_whisper = None;
        }

        action_save.set_state(&"custom".to_variant());

        show_status(&status_save, "自定义 API 模式");
        let st = status_save.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
            hide_status(&st);
        });

        dialog_save.close();
    });

    dialog.present();
}

// ── Window position ──────────────────────────────────────────────────────────

fn position_window(window: &gtk4::ApplicationWindow, db: &Arc<Mutex<Option<Db>>>) {
    let saved = if let Ok(d) = db.lock() {
        d.as_ref().and_then(|d| {
            let x = d.get_setting("window_x").ok().flatten().and_then(|s| s.parse::<i32>().ok());
            let y = d.get_setting("window_y").ok().flatten().and_then(|s| s.parse::<i32>().ok());
            x.zip(y)
        })
    } else { None };

    window.set_default_size(88, 100);

    if let Some((_x, _y)) = saved {
        // Position not restored in gtk4-rs 0.9.x (no move_to on Surface)
        window.present();
    } else {
        // Bottom-right
        if let Some(display) = gdk::Display::default() {
            if let Some(monitor) = display.monitors().item(0)
                .and_then(|m| m.downcast::<gdk::Monitor>().ok())
            {
                let geo = monitor.geometry();
                let x = geo.width() - 100;
                let y = geo.height() - 120;
                window.set_default_size(88, 100);
                window.present();
            } else {
                window.present();
            }
        }
    }
}

// ── Download helpers ─────────────────────────────────────────────────────────

fn download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let response = reqwest::blocking::get(url).map_err(|e| format!("下载失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().map_err(|e| format!("读取失败：{e}"))?;
    std::fs::write(dest, &bytes).map_err(|e| format!("保存失败：{e}"))?;
    Ok(())
}

fn download_funasr_binary(bin_dir: &std::path::Path) -> Result<(), String> {
    let url = config::FUNASR_BINARY.url;
    let response = reqwest::blocking::get(url).map_err(|e| format!("下载 FunASR 失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().map_err(|e| format!("读取压缩包失败：{e}"))?;
    let binary_name = config::FUNASR_BINARY.binary_name;
    let dest = bin_dir.join(binary_name);

    let cursor = std::io::Cursor::new(&bytes[..]);

    #[cfg(not(target_os = "windows"))]
    {
        let decoder = flate2::read::GzDecoder::new(cursor);
        let mut archive = tar::Archive::new(decoder);
        for result in archive.entries().map_err(|e| format!("读取压缩包失败：{e}"))? {
            let mut entry = result.map_err(|e| format!("解压项失败：{e}"))?;
            let name = entry.path().ok().and_then(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()));
            if let Some(ref name) = name {
                if name == binary_name || name.ends_with(binary_name) {
                    entry.unpack(&dest).map_err(|e| format!("解压失败：{e}"))?;
                    #[cfg(unix)] {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
                    }
                    return Ok(());
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: zip extraction
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("读取压缩包失败：{e}"))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| format!("读取项失败：{e}"))?;
            let name = entry.name().to_string();
            if name.ends_with(binary_name) || name.contains(binary_name) {
                let mut out = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败：{e}"))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| format!("写入文件失败：{e}"))?;
                return Ok(());
            }
        }
    }

    Err("未找到 FunASR 二进制".into())
}

// ── macOS window level ───────────────────────────────────────────────────────

// ── Window floating (set topmost / unset topmost) ─────────────────────────────

#[cfg(target_os = "macos")]
fn macos_set_window_floating(_window: &gtk4::ApplicationWindow) {
    unsafe {
        let ns_app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        let windows: *mut objc::runtime::Object = msg_send![ns_app, windows];
        let count: usize = msg_send![windows, count];
        for i in 0..count {
            let win: *mut objc::runtime::Object = msg_send![windows, objectAtIndex: i];
            let _: () = msg_send![win, setLevel: 7];
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_unset_window_floating(_window: &gtk4::ApplicationWindow) {
    unsafe {
        let ns_app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        let windows: *mut objc::runtime::Object = msg_send![ns_app, windows];
        let count: usize = msg_send![windows, count];
        for i in 0..count {
            let win: *mut objc::runtime::Object = msg_send![windows, objectAtIndex: i];
            let _: () = msg_send![win, setLevel: 0];
        }
    }
}

#[cfg(target_os = "windows")]
fn macos_set_window_floating(window: &gtk4::ApplicationWindow) {
    set_win32_topmost(window, true);
}

#[cfg(target_os = "windows")]
fn macos_unset_window_floating(window: &gtk4::ApplicationWindow) {
    set_win32_topmost(window, false);
}

#[cfg(target_os = "windows")]
fn set_win32_topmost(window: &gtk4::ApplicationWindow, topmost: bool) {
    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
        fn SetWindowPos(
            hWnd: isize, hWndInsertAfter: isize,
            X: i32, Y: i32, cx: i32, cy: i32, uFlags: u32,
        ) -> i32;
    }
    const HWND_TOPMOST: isize = -1;
    const HWND_NOTOPMOST: isize = -2;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;

    let title = window.title().unwrap_or_default();
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
        if hwnd != 0 {
            let insert = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
            SetWindowPos(hwnd, insert, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn macos_set_window_floating(_window: &gtk4::ApplicationWindow) {}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn macos_unset_window_floating(_window: &gtk4::ApplicationWindow) {}
