<h1 align="center">灵语（LingYu）v1.0.1</h1>

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
- **历史记录**：SQLite 存储 50 条最近识别记录，数据库损坏时显示错误提示
- **窗口置顶**：Win32 SetWindowPos 实现，首次启动自动保存右下角位置
- **调试日志**：所有错误写入 `C:\Users\<user>\lingyu-debug.log`

### v1.0.1 更新

- 修复：全局热键 Raw Input 解析偏移错误（vkey/flags 读取错位导致热键失效）
- 修复：提权进程 RIDEV_EXCLUDE 标志矛盾导致键盘事件被排除
- 修复：Helper 进程崩溃后共享内存 stale 状态导致持续录音
- 修复：剪贴板锁定导致转录结果丢失历史（DB 保存与剪贴板解耦）
- 修复：首次下载混合失败时显示误导性的"全部就绪"状态
- 修复：模型下载中麦克风不可用导致录音报错
- 修复：菜单在 FunASR 未安装时仍显示本地模型选项
- 优化：临时 WAV 文件使用 PID+计数器保证并发录制不冲突
- 优化：F16 模型检测改为精确文件名匹配，避免误判自定义路径
- 优化：剪贴板操作增加 5 次重试机制
- 优化：API Key 为空时不发送空 Bearer token
- 优化：ZIP 解压严格校验 .exe 后缀，防止提取校验和文件
- 优化：窗口位置首次启动自动保存到数据库
- 优化：UTF-8 字符串截断按字符边界进行，避免无效编码

---

## 快速开始

### 1、直接下载

从 [Releases](https://github.com/your/lingyu/releases) 下载 `lingyu-v1.0.1-win64.zip`，解压到任意空文件夹，双击 `lingyu.exe` 即可使用。

首次启动会自动下载 FunASR 引擎（`llama-funasr-sensevoice.exe`）和 VAD 模型到 `%LOCALAPPDATA%\\lingyu\\`。选择本地引擎后会自动下载对应的 SenseVoice GGUF 模型。

首次启动会弹出 **UAC 提权对话框**（全局热键需要），点是即可。如果拒绝，GTK 子类化回退方案仍然在普通窗口下可用。

### 2、自主构建

需要 [MSYS2](https://www.msys2.org/)，打开 **MINGW64 终端**安装依赖：

```bash
pacman -S mingw-w64-x86_64-gtk4 mingw-w64-x86_64-pkgconf mingw-w64-x86_64-gcc mingw-w64-x86_64-rust mingw-w64-x86_64-ntldd mingw-w64-x86_64-libwinpthread
git clone https://github.com/your/lingyu.git
cd lingyu
cargo build --release
```

> `build.rs` 会自动将 MSYS2 MINGW64 的 GTK4 DLL 复制到 `target/release/`，构建完成后 `.exe` 开箱即用。

构建产物：

```bash
./target/release/lingyu.exe --debug   # 首次建议加 --debug 查看日志
```

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

---

## 使用说明

| 操作 | 说明 |
|------|------|
| 按住左键 | 开始录音，松开自动识别并复制结果 |
| 按住快捷键（默认 F10，全局） | 同左键功能，桌面/普通窗口/管理员窗口/游戏全场景可用 |
| 右键菜单 | 切换本地模型（Q8/F16）、设置快捷键、窗口置顶、历史记录 |

### 录音流程

1. **按住**鼠标左键或快捷键 → 按钮变绿 + 图标切换 → 开始录音
2. **松手** → 自动停止录音并识别
3. 识别结果自动复制到剪贴板，状态栏显示"已复制"
4. 转录记录同时保存到 SQLite 历史数据库（与剪贴板独立）

### 全局热键工作原理

```
lingyu.exe（正常启动）
├── GTK 子类化 + RegisterRawInputDevices → 普通窗口场景（cmd、Chrome 等）
├── 启动时 ShellExecuteExW("runas") → UAC 提权
│   └── lingyu.exe --helper（提权进程）
│       ├── 隐藏窗口 + RegisterRawInputDevices → 桌面/管理员/游戏场景
│       └── 共享内存（LingyuHotkeyState）← 状态回传主进程
└── 30ms 定时器：读共享内存 + AtomicBool → 边沿检测 hold-to-talk
    └── Helper 崩溃自动检测 → 降级到 GTK 子类回退路径
```

### 切换模型

右键 → 语音转文字 → 选择 Q8（快速）或 F16（高精度），选中项前会有 ✔ 标记。

- 模型文件不存在时自动下载，下载期间麦克风不可用（显示"模型下载中，请稍候..."）
- FunASR 引擎未安装时菜单显示"本地引擎未安装"提示

### 快捷键

右键 → 设置快捷键 → 单选按钮选择 F1-F12 → 确定。自动写入 `shortcut.txt`（放在 `lingyu.exe` 同目录），GTK 子类化 + helper 同时生效。

默认：F10。修改后无需重启。

---

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

---

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

---

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

### 模型下载失败

检查网络连接，或手动下载模型文件（见上方"手动下载模型"表格）。下载失败时状态栏会显示具体错误信息。

### 历史记录打不开

如果数据库损坏，历史记录对话框会显示"数据库可能已损坏"提示。可尝试删除 `%LOCALAPPDATA%\lingyu\history.db` 后重启应用。

---

## 许可证

MIT License

---

*本软件由 AI 辅助编写。*
