# familiar-ai-gui

A desktop app that gives an AI a body — camera (eyes & neck), voice, robot legs, and episodic memory.
Built with Tauri + React + Rust.

> 📖 [日本語](README-ja.md) | [中文](README-zh.md) | [繁體中文](README-zh-TW.md) | [Français](README-fr.md) | [Deutsch](README-de.md)

## Features

- **Multi-LLM** — Kimi (Moonshot) / Claude (Anthropic) / Gemini (Google) / GPT (OpenAI)
- **Eyes & neck** — ONVIF PTZ camera for vision and pan/tilt (`see` / `look`)
- **Voice** — ElevenLabs TTS with real-time speech (`say`)
- **Legs** — Tuya robot vacuum for locomotion (`walk`)
- **Memory** — Episodic memory via SQLite + 384-dim embedding vectors (`remember` / `recall`)
- **Desire system** — Intrinsic motivation: the AI acts spontaneously when desires grow strong

Camera, TTS, and mobility are all optional — only an LLM API key is required to run.

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tauri CLI v2 | 2.x | `cargo install tauri-cli --version "^2"` |
| ffmpeg | any | Required only for RTSP camera snapshots |

---

## Setup & Run

```bash
# 1. Clone
git clone https://github.com/lifemate-ai/familiar-ai-gui.git
cd familiar-ai-gui

# 2. Install frontend dependencies
npm install

# 3. Start in development mode (hot reload)
npm run tauri dev
```

A setup wizard opens on first launch — enter your LLM API key and persona (3 steps).

### Production build

```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/
#   Linux:   .AppImage / .deb
#   macOS:   .dmg
#   Windows: .msi / .exe
```

---

## Configuration

Settings are saved by the wizard, but can also be edited directly.

**Location:**
- Linux/macOS: `~/.config/familiar-ai/config.toml`
- Windows: `%APPDATA%\familiar-ai\config.toml`

```toml
platform = "kimi"          # kimi | anthropic | gemini | openai
api_key = "sk-..."         # LLM API key (required)
model = ""                 # Leave empty for platform default (see table below)
agent_name = "Kokone"      # AI's name
persona = "..."            # Persona description injected into system prompt
companion_name = "Kouta"   # Your name

# ONVIF PTZ camera (optional)
[camera]
host = "192.168.1.100"
username = "admin"
password = "password"
onvif_port = 2020

# ElevenLabs TTS (optional)
[tts]
elevenlabs_api_key = "sk_..."
voice_id = "cgSgspJ2msm6clMCkdW9"

# Tuya robot vacuum (optional)
[mobility]
tuya_region = "us"         # us | eu | in
tuya_api_key = "..."
tuya_api_secret = "..."
tuya_device_id = "..."
```

### Default models by platform

| platform | default model |
|----------|--------------|
| `kimi` | `kimi-k2.5` |
| `anthropic` | `claude-haiku-4-5-20251001` |
| `gemini` | `gemini-2.5-flash` |
| `openai` | `gpt-4o-mini` |

---

## Tools

The agent can use the following tools:

| Tool | Args | Description |
|------|------|-------------|
| `see` | — | Capture a camera snapshot and show it to the AI |
| `look` | `direction` (left/right/up/down/around), `degrees` (1–90) | Pan/tilt the camera |
| `say` | `text`, `speaker` (camera/pc/both) | Speak aloud via ElevenLabs TTS |
| `walk` | `direction` (forward/backward/left/right/stop), `duration` (s, optional) | Move the robot vacuum |
| `remember` | `content`, `emotion`, `image_path` (optional) | Save an episodic memory |
| `recall` | `query`, `n` (count) | Semantic memory search |

---

## Data

| Data | Path |
|------|------|
| Config | `~/.config/familiar-ai/config.toml` |
| Memory database | `~/.familiar_ai/observations.db` (SQLite) |

---

## Testing

```bash
cd src-tauri
cargo test --lib
# → 201 tests passing
```

---

## Architecture

```
React frontend (Vite)
    ↕ Tauri IPC (invoke / event)
Rust backend
    ├── agent.rs        — ReAct agent loop + desire-driven idle ticks
    ├── desires.rs      — Desire system (observe_room / look_outside /
    │                     browse_curiosity / miss_companion)
    ├── backend/        — Multi-LLM adapters
    │   ├── kimi.rs
    │   ├── anthropic.rs
    │   ├── gemini.rs
    │   └── openai.rs
    └── tools/
        ├── camera.rs   — ONVIF PTZ + RTSP snapshot
        ├── tts.rs      — ElevenLabs TTS + Tapo camera speaker
        ├── tapo_audio.rs — Tapo HTTP Stream audio backchannel
        ├── mobility.rs — Tuya API (HMAC-SHA256 signing)
        └── memory.rs   — SQLite + fastembed embedding vectors
```

The agent runs a ReAct loop: build world model → recall memories → LLM streaming → execute tools → feedback → repeat.

A heartbeat thread fires an idle tick every 60 seconds when a desire exceeds the action threshold, enabling spontaneous behaviour without user input.

---

## IDE Setup

[VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
