# familiar-ai-gui

> 📖 [English](README.md) | [日本語](README-ja.md) | 简体中文 → [README-zh.md](README-zh.md) | [Français](README-fr.md) | [Deutsch](README-de.md)

一款為 AI 賦予身體的桌面應用——攝影機（眼睛與頸部）、語音、機器人腿和情節記憶。
基於 Tauri + React + Rust 構建。

## 功能

- **多 LLM 支援** — Kimi (Moonshot) / Claude (Anthropic) / Gemini (Google) / GPT (OpenAI)
- **眼睛與頸部** — ONVIF PTZ 攝影機用於視覺與雲台控制（`see` / `look`）
- **語音** — ElevenLabs TTS 即時語音合成（`say`）
- **腿部** — Tuya 掃地機器人用於移動（`walk`）
- **記憶** — 基於 SQLite + 384 維嵌入向量的情節記憶（`remember` / `recall`）
- **慾望系統** — 內在動機：慾望累積時 AI 會自發行動

攝影機、TTS 和移動功能均為可選——只需 LLM API 金鑰即可執行。

---

## 前置條件

| 工具 | 版本 | 安裝方式 |
|------|------|---------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tauri CLI v2 | 2.x | `cargo install tauri-cli --version "^2"` |
| ffmpeg | 任意 | 僅 RTSP 攝影機快照時需要 |

---

## 安裝與執行

```bash
git clone https://github.com/lifemate-ai/familiar-ai-gui.git
cd familiar-ai-gui
npm install
npm run tauri dev
```

首次啟動時會彈出設定精靈，輸入 LLM API 金鑰和人設（3 個步驟）。

### 生產建置

```bash
npm run tauri build
# 輸出: src-tauri/target/release/bundle/
```

---

## 設定檔

**儲存位置：**
- Linux/macOS: `~/.config/familiar-ai/config.toml`
- Windows: `%APPDATA%\familiar-ai\config.toml`

```toml
platform = "kimi"          # kimi | anthropic | gemini | openai
api_key = "sk-..."         # LLM API 金鑰（必填）
model = ""                 # 留空使用平台預設模型
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

| 工具 | 參數 | 說明 |
|------|------|------|
| `see` | — | 拍攝攝影機快照並傳送給 AI |
| `look` | `direction`, `degrees` | 控制攝影機方向 |
| `say` | `text`, `speaker` | 透過 ElevenLabs TTS 朗讀 |
| `walk` | `direction`, `duration` | 控制掃地機器人移動 |
| `remember` | `content`, `emotion` | 儲存情節記憶 |
| `recall` | `query`, `n` | 語意記憶檢索 |

---

## 測試

```bash
cd src-tauri && cargo test --lib
# → 201 tests passing
```
