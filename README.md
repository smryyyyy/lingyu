<h1 align="center">灵语（LingYu）v1.0.0</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.85+-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/GTK4-4.22-7F5AB6?style=flat-square&logo=gtk&logoColor=white" alt="GTK4">
  <img src="https://img.shields.io/badge/SenseVoice-FF6F00?style=flat-square&logo=huggingface&logoColor=white" alt="SenseVoice">
  <img src="https://img.shields.io/badge/Windows-0078D8?style=flat-square&logo=windows&logoColor=white" alt="Windows">
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="MIT License">
</p>

<p align="center">
  Windows 暗色浮窗语音转文字 — 按住录音，松手自动识别并复制到剪贴板。<br>
  <b>全局热键全场景可用：桌面、普通窗口、管理员窗口、游戏中。</b>
</p>

---

## 功能特点

- **暗色浮窗**：可选置顶的浮窗，SVG 麦克风图标
- **按住录音（Push-to-Talk）**：按住鼠标左键或 F10 录音，松手自动识别
- **全局热键全场景覆盖**：
  - GTK 窗口子类化 + Raw Input API（普通窗口）
  - 提权助手进程 + Raw Input API（桌面、管理员窗口、游戏）
  - 自动通过 ShellExecuteExW("runas") 提权，UAC 弹窗一次
- **本地模式**：调 `llama-funasr-sensevoice` 子进程 + SenseVoice GGUF 模型，离线识别
- **双模型支持**：SenseVoice Q8（~242MB，速度快）和 F16（~470MB，高精度），首次选择自动下载
- **VAD 检测**：集成 fsmn-vad 模型，自动过滤静音段
- **快捷键设置**：F1-F12 可选，写入 `shortcut.txt` 文件持久化，即时生效
- **历史记录**：SQLite 存储 50 条最近识别记录
- **窗口置顶**：Win32 SetWindowPos 实现
- **调试日志**：所有错误写入 `C:\Users\<user>\lingyu-debug.log`

## 快速开始

### 前置条件

- 安装 [MSYS2](https://www.msys2.org/)
- 打开 **MINGW64 终端**（不是 MSYS2、不是 UCRT64、不是 PowerShell）
- 安装依赖：

```bash
pacman -S mingw-w64-x86_64-gtk4 mingw-w64-x86_64-pkgconf mingw-w64-x86_64-gcc mingw-w64-x86_64-rust mingw-w64-x86_64-ntldd mingw-w64-x86_64-libwinpthread
```

### 从源码构建

```bash
git clone https://github.com/your/lingyu.git
cd lingyu
cargo build --release
```

> `build.rs` 会自动将 MSYS2 MINGW64 的 GTK4 DLL 复制到 `target/release/`，构建完成后 `.exe` 开箱即用。

### 首次启动

```bash
./target/release/lingyu.exe --debug
```

首次启动会自动下载 FunASR 引擎（`llama-funasr-sensevoice.exe`）和 VAD 模型到 `%LOCALAPPDATA%\lingyu\`。选择本地引擎后会自动下载对应的 SenseVoice GGUF 模型。

首次启动会弹出 **UAC 提权对话框**（全局热键需要），点是即可。如果拒绝，GTK 子类化回退方案仍然在普通窗口下可用。

### 手动下载模型

如果自动下载速度慢或网络受限，可手动下载后放入对应目录：

**模型文件：**

| 文件 | 大小 | 下载地址 | 存放路径 |
|------|------|----------|----------|
| VAD 模型 | ~1.7 MB | [fsmn-vad.gguf](https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf) | `%LOCALAPPDATA%\lingyu\models\fsmn-vad.gguf` |
| SenseVoice Q8 | ~242 MB | [sensevoice-small-q8.gguf](https://huggingface.co/FunAudioLLM/SenseVoiceSmall-GGUF/resolve/main/sensevoice-small-q8.gguf) | `%LOCALAPPDATA%\lingyu\models\sensevoice-small-q8.gguf` |
| SenseVoice F16 | ~470 MB | [sensevoice-small-f16.gguf](https://huggingface.co/FunAudioLLM/SenseVoiceSmall-GGUF/resolve/main/sensevoice-small-f16.gguf) | `%LOCALAPPDATA%\lingyu\models\sensevoice-small-f16.gguf` |
| FunASR 引擎 | ~1.4 MB | [funasr-llamacpp-windows-x64.zip](https://github.com/modelscope/FunASR/releases/download/runtime-llamacpp-v0.1.4/funasr-llamacpp-windows-x64.zip) | 解压后 `llama-funasr-sensevoice.exe` → `%LOCALAPPDATA%\lingyu\bin\` |

`%LOCALAPPDATA%` 通常是 `C:\Users\<用户名>\AppData\Local`。

手动放置后启动灵语，会自动跳过已存在的文件。

## 使用说明

| 操作 | 说明 |
|------|------|
| 按住左键 | 开始录音，松开自动识别并复制结果 |
| 按住 F10（全局） | 同左键功能，桌面/普通窗口/管理员窗口/游戏全场景可用 |
| 右键菜单 | 切换本地模型（Q8/F16）、设置快捷键、窗口置顶、历史记录 |

### 录音流程

1. **按住**鼠标左键或 F10 → 按钮变绿 + 图标切换 → 开始录音
2. **松手** → 自动停止录音并识别
3. 识别结果自动复制到剪贴板，状态栏显示"已复制"

### 全局热键工作原理

```
lingyu.exe（正常启动）
├── GTK 子类化 + RegisterRawInputDevices → 普通窗口场景（cmd、Chrome 等）
├── 启动时 ShellExecuteExW("runas") → UAC 提权
│   └── lingyu.exe --helper（提权进程）
│       ├── 隐藏窗口 + RegisterRawInputDevices → 桌面/管理员/游戏场景
│       └── 共享内存（LingyuHotkeyState）← 状态回传主进程
└── 30ms 定时器：读共享内存 + AtomicBool → 边沿检测 hold-to-talk
```

### 切换模型

右键 → 语音转文字 → 选择 Q8（快速）或 F16（高精度），选中项前会有 ✔ 标记。

### 快捷键

右键 → 设置快捷键 → 单选按钮选择 F1-F12 → 确定。自动写入 `shortcut.txt`（放在 `lingyu.exe` 同目录），GTK 子类化 + helper 同时生效。

默认：F10。修改后无需重启。

## 项目结构

```bash
.
├── src/
│   ├── main.rs         # 入口：正常模式 / --helper 提权模式
│   ├── ui.rs           # 核心 UI：浮窗、按钮、菜单、全局热键（GTK子类化+helper IPC）
│   ├── helper.rs       # 提权助手：隐藏窗口、Raw Input、共享内存 IPC
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
├── build.rs             # Windows DLL 自动打包（ntldd 递归解析 + gdk-pixbuf 加载器）
├── .cargo/
│   └── config.toml      # mingw64 linker + crt-static 配置
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
| rusqlite | SQLite 历史记录持久化 |
| arboard | 系统剪贴板 |
| gdk-pixbuf | SVG 图标渲染 |
| RegisterRawInputDevices + RIDEV_INPUTSINK | 全局热键（绕过 Vanguard） |
| ShellExecuteExW("runas") | 提权启动 helper 进程 |
| CreateFileMappingW + 共享内存 | 进程间热键状态通信 |
| SetWindowPos | Win32 窗口置顶 |

## 常见问题

### 全局热键在桌面/管理员窗口/游戏中无效

确认 UAC 提权对话框已点是。如已拒绝，可以手动删除 `%TEMP%\LingyuHotkeyState` 文件映射后重新启动。

### 没有弹出 UAC 提权对话框

手动运行一次提权模式：

```bash
lingyu.exe --helper
```

然后在正常模式启动即可。

### 我想使用快捷键，但不希望每次启动都弹出UAC

首次点是后，Windows 会记住该应用的提权请求。如果希望完全静默，可以创建一个计划任务以最高权限运行 `lingyu.exe --helper`。

## 许可证

MIT License

---

*本软件由 AI 辅助编写。*
