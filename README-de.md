# familiar-ai-gui

> 📖 [English](README.md) | [日本語](README-ja.md) | [中文](README-zh.md) | [繁體中文](README-zh-TW.md) | [Français](README-fr.md)

Eine Desktop-App, die einer KI einen Körper gibt — Kamera (Augen & Hals), Stimme, Roboterbeine und episodisches Gedächtnis.
Gebaut mit Tauri + React + Rust.

## Funktionen

- **Multi-LLM** — Kimi (Moonshot) / Claude (Anthropic) / Gemini (Google) / GPT (OpenAI)
- **Augen & Hals** — ONVIF PTZ-Kamera für Sicht und Schwenk/Neigung (`see` / `look`)
- **Stimme** — ElevenLabs TTS mit Echtzeit-Sprachausgabe (`say`)
- **Beine** — Tuya-Saugroboter für Fortbewegung (`walk`)
- **Gedächtnis** — Episodisches Gedächtnis via SQLite + 384-dim. Einbettungsvektoren (`remember` / `recall`)
- **Wunschsystem** — Intrinsische Motivation: Die KI handelt spontan, wenn Wünsche anwachsen

Kamera, TTS und Mobilität sind alle optional — nur ein LLM-API-Schlüssel ist erforderlich.

---

## Voraussetzungen

| Tool | Version | Installation |
|------|---------|-------------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tauri CLI v2 | 2.x | `cargo install tauri-cli --version "^2"` |
| ffmpeg | beliebig | Nur für RTSP-Kamera-Snapshots erforderlich |

---

## Installation und Start

```bash
git clone https://github.com/lifemate-ai/familiar-ai-gui.git
cd familiar-ai-gui
npm install
npm run tauri dev
```

Beim ersten Start öffnet sich ein Setup-Assistent — geben Sie Ihren LLM-API-Schlüssel und die Persona ein (3 Schritte).

### Produktions-Build

```bash
npm run tauri build
# Ausgabe: src-tauri/target/release/bundle/
```

---

## Konfiguration

**Speicherort:**
- Linux/macOS: `~/.config/familiar-ai/config.toml`
- Windows: `%APPDATA%\familiar-ai\config.toml`

```toml
platform = "kimi"          # kimi | anthropic | gemini | openai
api_key = "sk-..."         # LLM-API-Schlüssel (erforderlich)
model = ""                 # Leer lassen für Plattform-Standard
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

## Verfügbare Tools

| Tool | Parameter | Beschreibung |
|------|-----------|-------------|
| `see` | — | Kamera-Snapshot aufnehmen und KI zeigen |
| `look` | `direction`, `degrees` | Kamerarichtung steuern |
| `say` | `text`, `speaker` | Text via ElevenLabs TTS sprechen |
| `walk` | `direction`, `duration` | Saugroboter bewegen |
| `remember` | `content`, `emotion` | Episodische Erinnerung speichern |
| `recall` | `query`, `n` | Semantische Gedächtnissuche |

---

## Tests

```bash
cd src-tauri && cargo test --lib
# → 201 tests passing
```
