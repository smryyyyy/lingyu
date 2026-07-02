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
    @keyframes pulse {
        0%   { opacity: 1.0; }
        50%  { opacity: 0.7; }
        100% { opacity: 1.0; }
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
            .unwrap_or_else(|| config::LOCAL_MODEL_PRESETS.first().map(|m| m.id).unwrap_or("sensevoice-q8").to_string())
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

    // ── Layout ──
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_halign(gtk4::Align::Center);
    vbox.set_valign(gtk4::Align::Center);

    // ── Button (SVG mic icon) ──
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
    vbox.append(&status);

    let handle = gtk4::WindowHandle::new();
    handle.set_child(Some(&vbox));

    window.set_child(Some(&handle));

    // ── Left-click handler (push-to-talk) ──
    let gesture_left = gtk4::GestureClick::new();
    gesture_left.set_button(1);
    gesture_left.set_exclusive(true);

    // Shared recording start helper
    let rec_start = {
        let state_s = Rc::clone(&state);
        let recorder_s = Rc::clone(&recorder);
        let button_s = button.clone();
        let icon_s = icon.clone();
        let mic_rec_pixbuf_s = mic_rec_pixbuf.clone();
        let status_s = status.clone();
        move || -> bool {
            if *state_s.borrow() != State::Idle { return false; }
            if let Err(e) = recorder_s.borrow_mut().start() {
                log_error(&format!("录音失败：{e}"));
                show_status(&status_s, "错误，看日志");
                return false;
            }
            *state_s.borrow_mut() = State::Recording;
            icon_s.set_from_pixbuf(Some(&mic_rec_pixbuf_s));
            button_s.add_css_class("recording");
            show_status(&status_s, "录音中...");
            true
        }
    };

    // Shared recording stop + transcribe helper
    let rec_stop = {
        let state_s = Rc::clone(&state);
        let recorder_s = Rc::clone(&recorder);
        let icon_s = icon.clone();
        let mic_pixbuf_s = mic_pixbuf.clone();
        let button_s = button.clone();
        let status_s = status.clone();
        let runtime_s = Rc::clone(&runtime);
        let db_s = Arc::clone(&db);
        move || {
            if *state_s.borrow() != State::Recording { return; }
            let wav_data = match recorder_s.borrow_mut().stop() {
                Ok(d) => d,
                Err(e) => {
                    *state_s.borrow_mut() = State::Idle;
                    icon_s.set_from_pixbuf(Some(&mic_pixbuf_s));
                    button_s.remove_css_class("recording");
                    hide_status(&status_s);
                    show_status(&status_s, &format!("{e}"));
                    return;
                }
            };
            *state_s.borrow_mut() = State::Processing;
            icon_s.set_from_pixbuf(Some(&mic_pixbuf_s));
            button_s.remove_css_class("recording");
            show_status(&status_s, "识别中...");

            let state_p = Rc::clone(&state_s);
            let button_p = button_s.clone();
            let runtime_p = Rc::clone(&runtime_s);
            let db_p = Arc::clone(&db_s);
            let status_p = status_s.clone();
            let sample_rate = recorder_s.borrow().sample_rate();

            let (mode_is_api, api_key_s, api_url_s, api_model_s, local_whisper_s) = {
                let rt = runtime_p.borrow();
                let is_api = matches!(rt.active_service, TranscriptionService::Api);
                let key = rt.api_key.clone().unwrap_or_default();
                let url = rt.api_base_url.clone();
                let mdl = rt.api_model.clone();
                let local = rt.local_whisper.clone();
                (is_api, key, url, mdl, local)
            };

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
                    button_p.remove_css_class("recording");
                }
                glib::ControlFlow::Break
            });
        }
    };

    let rec_start_g = rec_start.clone();
    let rec_stop_g = rec_stop.clone();
    gesture_left.connect_pressed(move |g, _n_press, _x, _y| {
        g.set_state(gtk4::EventSequenceState::Claimed);
        rec_start_g();
    });
    gesture_left.connect_released(move |g, _n_press, _x, _y| {
        g.set_state(gtk4::EventSequenceState::Claimed);
        rec_stop_g();
    });
    button.add_controller(gesture_left);

    // ── Right-click menu ──
    let stt_section = gtk4::gio::Menu::new();

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
        if new_state { set_win32_topmost(&win_aat, true); }
        else { set_win32_topmost(&win_aat, false); }
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
        let _ = std::process::Command::new("explorer").arg(&path_str).spawn();
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

    // ── Global hotkey polling (GetAsyncKeyState) ──
    {
        let vk_code: i32 = match sc_key.as_str() {
            "F1" => 0x70, "F2" => 0x71, "F3" => 0x72, "F4" => 0x73,
            "F5" => 0x74, "F6" => 0x75, "F7" => 0x76, "F8" => 0x77,
            "F9" => 0x78, "F10" => 0x79, "F11" => 0x7A, "F12" => 0x7B,
            _ => 0x75,
        };
        extern "system" {
            fn GetAsyncKeyState(vKey: i32) -> i16;
        }
        let was_down = Rc::new(std::cell::RefCell::new(false));
        let was_down_hk = Rc::clone(&was_down);
        let state_hk = Rc::clone(&state);
        let recorder_hk = Rc::clone(&recorder);
        let button_hk = button.clone();
        let status_hk = status.clone();
        let runtime_hk = Rc::clone(&runtime);
        let db_hk = Arc::clone(&db);
        glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
            let now_down = unsafe { GetAsyncKeyState(vk_code) as u16 & 0x8000 != 0 };
            let prev_down = *was_down_hk.borrow();
            *was_down_hk.borrow_mut() = now_down;

            // Push-to-talk: key down → start, key up → stop
            if now_down && !prev_down && *state_hk.borrow() == State::Idle {
                if let Err(e) = recorder_hk.borrow_mut().start() {
                    log_error(&format!("全局热键录音失败：{e}"));
                    show_status(&status_hk, "错误，看日志");
                    return glib::ControlFlow::Continue;
                }
                *state_hk.borrow_mut() = State::Recording;
                button_hk.add_css_class("recording");
                show_status(&status_hk, "录音中...");
            } else if !now_down && prev_down && *state_hk.borrow() == State::Recording {
                stop_and_transcribe(
                    &state_hk, &recorder_hk, &button_hk,
                    &status_hk, &db_hk, &runtime_hk,
                );
            }
            glib::ControlFlow::Continue
        });
    }

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
    set_win32_topmost(&window, true);
}

// ── Shared stop + transcribe helper ─────────────────────────────────────────

/// Stop the recorder, transcribe in a background thread, and update UI.
/// Used by both the global hotkey timer and the gesture handlers.
fn stop_and_transcribe(
    state: &Rc<std::cell::RefCell<State>>,
    recorder: &Rc<std::cell::RefCell<Recorder>>,
    button: &gtk4::Button,
    status: &gtk4::Label,
    db: &Arc<Mutex<Option<Db>>>,
    runtime: &Rc<std::cell::RefCell<RuntimeState>>,
) {
    if *state.borrow() != State::Recording { return; }
    let wav_data = match recorder.borrow_mut().stop() {
        Ok(d) => d,
        Err(e) => {
            *state.borrow_mut() = State::Idle;
            button.remove_css_class("recording");
            hide_status(status);
            show_status(status, &format!("{e}"));
            return;
        }
    };
    *state.borrow_mut() = State::Processing;
    button.remove_css_class("recording");
    show_status(status, "识别中...");

    let state_p = Rc::clone(state);
    let button_p = button.clone();
    let runtime_p = Rc::clone(runtime);
    let db_p = Arc::clone(db);
    let status_p = status.clone();
    let sample_rate = recorder.borrow().sample_rate();

    let (mode_is_api, api_key_s, api_url_s, api_model_s, local_whisper_s) = {
        let rt = runtime_p.borrow();
        let is_api = matches!(rt.active_service, TranscriptionService::Api);
        let key = rt.api_key.clone().unwrap_or_default();
        let url = rt.api_base_url.clone();
        let mdl = rt.api_model.clone();
        let local = rt.local_whisper.clone();
        (is_api, key, url, mdl, local)
    };

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
                    std::time::Duration::from_secs(2), move || hide_status(&st));
            }
            Ok(Err(e)) => {
                log_error(&format!("识别失败：{e}"));
                show_status(&status_p, "错误，看日志");
                let st = status_p.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_secs(4), move || hide_status(&st));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(_) => {}
        }
        if *state_p.borrow() == State::Processing {
            *state_p.borrow_mut() = State::Idle;
            button_p.remove_css_class("recording");
        }
        glib::ControlFlow::Break
    });
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

    // Zip extraction
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

    Err("未找到 FunASR 二进制".into())
}

// ── Window floating (set topmost / unset topmost) ─────────────────────────────

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
