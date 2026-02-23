# familiar-ai-gui

> 📖 [English](README.md) | [日本語](README-ja.md) | [中文](README-zh.md) | [繁體中文](README-zh-TW.md) | [Deutsch](README-de.md)

Une application de bureau qui donne un corps à une IA — caméra (yeux et cou), voix, jambes robotiques et mémoire épisodique.
Construit avec Tauri + React + Rust.

## Fonctionnalités

- **Multi-LLM** — Kimi (Moonshot) / Claude (Anthropic) / Gemini (Google) / GPT (OpenAI)
- **Yeux et cou** — Caméra ONVIF PTZ pour la vision et le panoramique/inclinaison (`see` / `look`)
- **Voix** — Synthèse vocale ElevenLabs en temps réel (`say`)
- **Jambes** — Aspirateur robot Tuya pour la locomotion (`walk`)
- **Mémoire** — Mémoire épisodique via SQLite + vecteurs d'embedding 384 dimensions (`remember` / `recall`)
- **Système de désirs** — Motivation intrinsèque : l'IA agit spontanément quand les désirs s'accumulent

La caméra, TTS et la mobilité sont tous optionnels — seule une clé API LLM est nécessaire.

---

## Prérequis

| Outil | Version | Installation |
|-------|---------|-------------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tauri CLI v2 | 2.x | `cargo install tauri-cli --version "^2"` |
| ffmpeg | quelconque | Requis uniquement pour les snapshots de caméra RTSP |

---

## Installation et démarrage

```bash
git clone https://github.com/lifemate-ai/familiar-ai-gui.git
cd familiar-ai-gui
npm install
npm run tauri dev
```

Un assistant de configuration s'ouvre au premier lancement — entrez votre clé API LLM et le persona (3 étapes).

### Build de production

```bash
npm run tauri build
# Sortie : src-tauri/target/release/bundle/
```

---

## Configuration

**Emplacement :**
- Linux/macOS : `~/.config/familiar-ai/config.toml`
- Windows : `%APPDATA%\familiar-ai\config.toml`

```toml
platform = "kimi"          # kimi | anthropic | gemini | openai
api_key = "sk-..."         # Clé API LLM (obligatoire)
model = ""                 # Laisser vide pour le modèle par défaut
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

## Outils disponibles

| Outil | Arguments | Description |
|-------|-----------|-------------|
| `see` | — | Capture une image et la montre à l'IA |
| `look` | `direction`, `degrees` | Oriente la caméra |
| `say` | `text`, `speaker` | Synthèse vocale via ElevenLabs |
| `walk` | `direction`, `duration` | Déplace l'aspirateur robot |
| `remember` | `content`, `emotion` | Sauvegarde une mémoire épisodique |
| `recall` | `query`, `n` | Recherche sémantique dans les souvenirs |

---

## Tests

```bash
cd src-tauri && cargo test --lib
# → 201 tests passing
```
