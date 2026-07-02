<h1 align="center">灵语（LingYu）</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.85+-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/GTK4-4.22-7F5AB6?style=flat-square&logo=gtk&logoColor=white" alt="GTK4">
  <img src="https://img.shields.io/badge/SenseVoice-FF6F00?style=flat-square&logo=huggingface&logoColor=white" alt="SenseVoice">
  <img src="https://img.shields.io/badge/Windows-0078D6?style=flat-square&logo=windows&logoColor=white" alt="Windows">
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="MIT License">
</p>

<p align="center">
  Windows 暗色浮窗语音转文字 — 按住录音，松手自动识别并复制到剪贴板。
</p>

---

## 目录

- [功能特点](#功能特点)
- [快速开始](#快速开始)
- [使用说明](#使用说明)
- [项目结构](#项目结构)
- [技术栈](#技术栈)
- [许可证](#许可证)

---

## 功能特点

- **暗色浮窗**：可选置顶的浮窗，SVG 麦克风图标
- **按住录音（Push-to-Talk）**：按住鼠标左键或 F6 录音，松手自动识别
- **全局热键**：F6 全局生效，切到其他窗口也能按住录音
- **本地模式**：调 `llama-funasr-sensevoice` 子进程 + SenseVoice GGUF 模型，离线识别
- **双模型支持**：SenseVoice Q8（~242MB，速度快）和 F16（~470MB，精度高），首次选择自动下载
- **VAD 检测**：集成 fsmn-vad 模型，自动过滤静音段
- **快捷键设置**：F1-F12 可选，DB 持久化
- **历史记录**：SQLite 存储 50 条最近识别记录
- **窗口置顶**：Win32 SetWindowPos 实现
- **调试日志**：所有错误写入 `C:\Users\<user>\lingyu-debug.log`

## 快速开始

### 前置条件

- 安装 [MSYS2](https://www.msys2.org/)
- 打开 **UCRT64 终端**（不是 MSYS2、不是 MINGW64、不是 PowerShell）
- 安装依赖：

```bash
pacman -S mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-rust
```

### 从源码构建

```bash
git clone https://github.com/your/lingyu.git
cd lingyu
cargo build --release
```

> `build.rs` 会自动将 MSYS2 UCRT64 的 GTK4 DLL 复制到 `target/release/`，构建完成后 `.exe` 开箱即用。

### 首次启动

```bash
./target/release/lingyu.exe --debug
```

首次启动会自动下载 FunASR 引擎（`llama-funasr-sensevoice.exe`）和 VAD 模型到 `%LOCALAPPDATA%\lingyu\`。选择本地引擎后会自动下载对应的 SenseVoice GGUF 模型。

## 使用说明

| 操作 | 说明 |
|------|------|
| 按住左键 | 开始录音，松开自动识别并复制结果 |
| 右键菜单 | 切换本地模型（Q8/F16）、设置快捷键、窗口置顶、历史记录 |
| 全局 F6 | 同左键功能，切到其他窗口也生效 |

### 录音流程

1. **按住**鼠标左键或 F6 → 按钮变绿 + 图标切换 → 开始录音
2. **松手** → 自动停止录音并识别
3. 识别结果自动复制到剪贴板，状态栏显示"已复制"

### 切换模型

右键 → 语音转文字 → 选择 Q8（快速）或 F16（高精度），选中项前会有 ✔ 标记。

### 快捷键

右键 → 设置快捷键 → 选择 F1-F12 → 确定。全局热键随设置变更（下次启动生效）。

## 项目结构

```bash
.
├── src/
│   ├── main.rs         # 入口，应用初始化
│   ├── ui.rs           # 核心 UI：浮窗、按钮、菜单、全局热键、下载
│   ├── config.rs       # 配置加载、模型预设、FunASR/VAD 信息
│   ├── audio.rs        # cpal 录音 + hound WAV 编码
│   ├── local_stt.rs    # FunASR 子进程调用 + 输出解析
│   ├── api.rs          # OpenAI 兼容 STT API（reqwest blocking）
│   ├── db.rs           # SQLite 历史 + 设置键值存储
│   ├── input.rs        # 剪贴板（arboard）
│   ├── log.rs          # 调试日志模块
│   └── tests/
│       └── mod.rs      # 测试模块
├── icons/
│   ├── microphone.svg             # 白色麦克风（空闲）
│   └── microphone_recording.svg   # 红色方块（录音中）
├── build.rs             # Windows 自动打包 GTK4 DLL + gdk-pixbuf 加载器
├── Cargo.toml           # 依赖清单
└── README.md
```

## 技术栈

| 组件 | 用途 |
|------|------|
| Rust + GTK4 | 桌面 UI（暗色浮窗 + PopoverMenu） |
| cpal + hound | 实时录音 + WAV 编码 |
| llama-funasr-sensevoice | FunASR 推理引擎（llama.cpp 后端） |
| SenseVoice GGUF | 语音识别模型（Q8 / F16） |
| reqwest (blocking) | API 请求 + 模型下载 |
| rusqlite | SQLite 历史 + 设置持久化 |
| arboard | 系统剪贴板 |
| gdk-pixbuf | SVG 图标渲染 |
| GetAsyncKeyState | Windows 全局热键轮询 |
| SetWindowPos | Win32 窗口置顶 |

## 许可证

MIT License
