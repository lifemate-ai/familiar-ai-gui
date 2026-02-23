# familiar-ai-gui

> 📖 [English README](README.md)

AIに身体を与えるデスクトップアプリ。カメラ（目・首）・音声（声）・ロボット掃除機（足）・エピソード記憶を持つ AI コンパニオンを動かすための Tauri + React + Rust アプリ。

## Features

- **マルチ LLM 対応** — Kimi (Moonshot) / Claude (Anthropic) / Gemini (Google) / GPT (OpenAI)
- **目・首** — ONVIF PTZ カメラで撮影・首振り（`see` / `look`）
- **声** — ElevenLabs TTS でリアルタイム発話（`say`）
- **足** — Tuya ロボット掃除機で移動（`walk`）
- **記憶** — SQLite + 384 次元埋め込みベクトルによるエピソード記憶（`remember` / `recall`）
- **欲求システム** — 内発的動機付けによる自律的な探索行動

カメラ・TTS・移動はすべてオプションなので、API キーだけあれば動く。

---

## Prerequisites（前提条件）

| ツール | バージョン | インストール |
|--------|-----------|-------------|
| Node.js | 18+ | [nodejs.org](https://nodejs.org/) |
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tauri CLI v2 | 2.x | `cargo install tauri-cli --version "^2"` |
| ffmpeg | 任意 | RTSP カメラ使用時のみ必要 |

---

## Setup & Run

```bash
# 1. リポジトリのクローン
git clone https://github.com/lifemate-ai/familiar-gui.git
cd familiar-gui

# 2. フロントエンド依存関係のインストール
npm install

# 3. 開発モードで起動（ホットリロード）
npm run tauri dev
```

初回起動時にセットアップウィザードが開くので、LLM の API キーとペルソナを設定する（3 ステップ）。

### プロダクションビルド

```bash
npm run tauri build
# 成果物: src-tauri/target/release/bundle/
#   Linux:   .AppImage / .deb
#   macOS:   .dmg
#   Windows: .msi / .exe
```

---

## Configuration（設定ファイル）

設定は GUI のウィザードで保存されるが、直接編集も可能。

**保存先:**
- Linux/macOS: `~/.config/familiar-ai/config.toml`
- Windows: `%APPDATA%\familiar-ai\config.toml`

```toml
platform = "kimi"          # kimi | anthropic | gemini | openai
api_key = "sk-..."         # LLM API キー（必須）
model = ""                 # 省略時はプラットフォームのデフォルト（下表参照）
agent_name = "Kokone"      # AI の名前
persona = "..."            # システムプロンプトに挿入されるペルソナ説明
companion_name = "Kouta"   # 人間側の名前

# ONVIF PTZ カメラ（任意）
[camera]
host = "192.168.1.100"
username = "admin"
password = "password"
onvif_port = 2020

# ElevenLabs TTS（任意）
[tts]
elevenlabs_api_key = "sk_..."
voice_id = "cgSgspJ2msm6clMCkdW9"

# Tuya ロボット掃除機（任意）
[mobility]
tuya_region = "us"         # us | eu | in
tuya_api_key = "..."
tuya_api_secret = "..."
tuya_device_id = "..."
```

### プラットフォーム別デフォルトモデル

| platform | デフォルトモデル |
|----------|----------------|
| `kimi` | `kimi-k2.5` |
| `anthropic` | `claude-haiku-4-5-20251001` |
| `gemini` | `gemini-2.5-flash` |
| `openai` | `gpt-4o-mini` |

---

## Tools（エージェントが使えるツール）

| ツール | 引数 | 説明 |
|--------|------|------|
| `see` | なし | カメラで撮影し AI に見せる |
| `look` | `direction` (left/right/up/down/around), `degrees` (1-90) | カメラの向きを変える |
| `say` | `text` | ElevenLabs で音声合成・発話（1〜2文） |
| `walk` | `direction` (forward/backward/left/right/stop), `duration` (秒, 任意) | ロボット掃除機で移動 |
| `remember` | `content`, `emotion`, `image_path` (任意) | エピソード記憶を保存 |
| `recall` | `query`, `n` (件数) | 記憶を意味検索 |

---

## Data（データ保存先）

| データ | パス |
|--------|------|
| 設定ファイル | `~/.config/familiar-ai/config.toml` |
| 記憶データベース | `~/.familiar_ai/observations.db` (SQLite) |

---

## Testing（テスト）

```bash
cd src-tauri
cargo test
# → 199 tests passing
```

---

## Architecture（アーキテクチャ）

```
React frontend (Vite)
    ↕ Tauri IPC (invoke / event)
Rust backend
    ├── agent.rs        — ReAct エージェントループ
    ├── desires.rs      — 欲求システム（内発的動機付け）
    ├── backend/        — マルチ LLM アダプター
    │   ├── kimi.rs
    │   ├── anthropic.rs
    │   ├── gemini.rs
    │   └── openai.rs
    └── tools/
        ├── camera.rs   — ONVIF PTZ + RTSP スナップショット
        ├── tts.rs      — ElevenLabs TTS
        ├── mobility.rs — Tuya API (HMAC-SHA256 署名)
        └── memory.rs   — SQLite + fastembed 埋め込みベクトル
```

エージェントは ReAct ループで動作する：世界モデルの構築 → 記憶の想起 → LLM ストリーミング → ツール実行 → フィードバック → 繰り返し。

---

## IDE Setup（推奨）

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
