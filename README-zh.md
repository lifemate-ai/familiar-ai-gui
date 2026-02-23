# familiar-ai-gui

> 📖 [English](README.md) | [日本語](README-ja.md) | 繁體中文 → [README-zh-TW.md](README-zh-TW.md) | [Français](README-fr.md) | [Deutsch](README-de.md)

一款为 AI 赋予身体的桌面应用——摄像头（眼睛与颈部）、语音、机器人腿和情节记忆。
基于 Tauri + React + Rust 构建。

## 功能

- **多 LLM 支持** — Kimi (Moonshot) / Claude (Anthropic) / Gemini (Google) / GPT (OpenAI)
- **眼睛与颈部** — ONVIF PTZ 摄像头用于视觉与云台控制（`see` / `look`）
- **语音** — ElevenLabs TTS 实时语音合成（`say`）
- **腿部** — Tuya 扫地机器人用于移动（`walk`）
- **记忆** — 基于 SQLite + 384 维嵌入向量的情节记忆（`remember` / `recall`）
- **欲望系统** — 内在动机：欲望积累时 AI 会自发行动

摄像头、TTS 和移动功能均为可选——只需 LLM API 密钥即可运行。

---

## 前置条件

| 工具 | 版本 | 安装方式 |
|------|------|---------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tauri CLI v2 | 2.x | `cargo install tauri-cli --version "^2"` |
| ffmpeg | 任意 | 仅 RTSP 摄像头快照时需要 |

---

## 安装与运行

```bash
git clone https://github.com/lifemate-ai/familiar-ai-gui.git
cd familiar-ai-gui
npm install
npm run tauri dev
```

首次启动时会弹出设置向导，输入 LLM API 密钥和人设（3 个步骤）。

### 生产构建

```bash
npm run tauri build
# 输出: src-tauri/target/release/bundle/
```

---

## 配置文件

**保存位置：**
- Linux/macOS: `~/.config/familiar-ai/config.toml`
- Windows: `%APPDATA%\familiar-ai\config.toml`

```toml
platform = "kimi"          # kimi | anthropic | gemini | openai
api_key = "sk-..."         # LLM API 密钥（必填）
model = ""                 # 留空使用平台默认模型
agent_name = "Kokone"
persona = "..."
companion_name = "Kouta"

[camera]
host = "192.168.1.100"
username = "admin"
password = "password"
onvif_port = 2020

[tts]
elevenlabs_api_key = "sk_..."
voice_id = "cgSgspJ2msm6clMCkdW9"

[mobility]
tuya_region = "us"         # us | eu | in
tuya_api_key = "..."
tuya_api_secret = "..."
tuya_device_id = "..."
```

---

## 可用工具

| 工具 | 参数 | 说明 |
|------|------|------|
| `see` | — | 拍摄摄像头快照并发送给 AI |
| `look` | `direction`, `degrees` | 控制摄像头方向 |
| `say` | `text`, `speaker` | 通过 ElevenLabs TTS 朗读 |
| `walk` | `direction`, `duration` | 控制扫地机器人移动 |
| `remember` | `content`, `emotion` | 保存情节记忆 |
| `recall` | `query`, `n` | 语义记忆检索 |

---

## 测试

```bash
cd src-tauri && cargo test --lib
# → 201 tests passing
```
