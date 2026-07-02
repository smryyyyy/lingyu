<h1 align="center">灵语（LingYu）</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.85+-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/GTK4-4.14-7F5AB6?style=flat-square&logo=gtk&logoColor=white" alt="GTK4">
  <img src="https://img.shields.io/badge/SenseVoice-FF6F00?style=flat-square&logo=huggingface&logoColor=white" alt="SenseVoice">
  <img src="https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white" alt="macOS">
  <img src="https://img.shields.io/badge/Windows-0078D6?style=flat-square&logo=windows&logoColor=white" alt="Windows">
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="MIT License">
</p>

<p align="center">
  浮窗语音转文字 — 左键录音，右键切引擎，识别结果自动复制到剪贴板。
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

- **浮窗**：可选置顶的浮窗，SVG 麦克风图标，空闲时白色，录音时切换为红色方块
- **左键录音**：点击录音/停止 → 异步识别 → 自动复制到剪贴板，全程不打断工作流
- **右键菜单**：切换本地模型 / 自定义 API / 置顶切换 / 快捷键设置 / 历史记录 / 退出
- **本地模式**：调 `llama-funasr-sensevoice` 子进程 + SenseVoice GGUF 模型，离线识别
- **API 模式**：自定义 OpenAI 兼容端点（URL + Key + 模型名），支持任意 Whisper 兼容服务
- **双模型支持**：SenseVoice Q8（~242MB，速度快）和 F16（~470MB，精度高），首次选择自动下载
- **VAD 检测**：集成 fsmn-vad 模型，自动过滤静音段
- **快捷键**：F1-F12 可选，DB 持久化，实时生效
- **历史记录**：SQLite 存储 50 条最近识别记录
- **跨平台**：macOS ARM64 / Windows x64
- **调试日志**：所有错误写入 `~/lingyu-debug.log`，UI 保持简洁

## 快速开始

### 前置条件

- Rust 工具链（`rustup` 安装）
- GTK4 及开发库（macOS: `brew install gtk4 pkgconf cmake`）

### 从源码构建

```bash
git clone https://github.com/your/lingyu.git
cd lingyu

# macOS 需设置 PKG_CONFIG_PATH
export PKG_CONFIG_PATH="/opt/homebrew/opt/glib/lib/pkgconfig:/opt/homebrew/opt/gtk4/lib/pkgconfig:$PKG_CONFIG_PATH"

# 构建
cargo build --release

# 运行
cargo run --release
```

### 首次启动

首次启动会自动下载 FunASR 引擎（`llama-funasr-sensevoice`）和 VAD 模型到 `~/Library/Application Support/lingyu/`。选择本地引擎后会自动下载对应的 SenseVoice GGUF 模型。

## 使用说明

| 操作 | 说明 |
|------|------|
| 左键点击图标 | 开始录音 / 停止录音（会自动识别并复制结果） |
| 右键点击图标 | 弹出菜单：切换引擎、设置快捷键、查看历史 |
| F6（可自定义） | 全局快捷键，同左键功能 |
| 设置 → 快捷键 | 打开对话框选择 F1-F12 |

### 录音 + 识别

1. 左键麦克风或按快捷键 → 开始录音（图标变红）
2. 再点左键或快捷键 → 停止录音，自动识别
3. 识别结果自动复制到剪贴板，状态栏显示"已复制"

### 切换引擎

右键 → 菜单分为两个区：

- **本地 SenseVoice**：Q8（快速低内存） / F16（高精度）
- **自定义 API**：输入任意 OpenAI 兼容的 STT 端点

选中项前会有 ✔ 标记，退出后自动保留选择。

## 项目结构

```bash
.
├── src/
│   ├── main.rs         # 入口，应用初始化
│   ├── ui.rs           # 核心 UI：浮窗、按钮、菜单、快捷键、下载
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
│   ├── microphone.svg             # 白色麦克风
│   └── microphone_recording.svg   # 红色方块
├── Cargo.toml          # 依赖清单
└── README.md
```

## 技术栈

| 组件 | 用途 |
|------|------|
| Rust + GTK4 | 桌面 UI（透明浮窗 + PopoverMenu） |
| cpal + hound | 实时录音 + WAV 编码 |
| llama-funasr-sensevoice | FunASR 推理引擎（llama.cpp 后端） |
| SenseVoice GGUF | 语音识别模型（Q8 / F16） |
| reqwest (blocking) | API 模式 HTTP 请求 |
| rusqlite | SQLite 历史 + 设置持久化 |
| arboard | 系统剪贴板 |
| objc (macOS) | 原生窗口置顶 API |

## 许可证

MIT License
