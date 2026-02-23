import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onComplete: () => void;
}

type Step = "llm" | "persona" | "hardware";

const PLATFORMS = [
  { id: "kimi", label: "Kimi K2.5", sub: "おすすめ・安い", url: "https://platform.moonshot.ai" },
  { id: "anthropic", label: "Claude (Anthropic)", sub: "高品質", url: "https://console.anthropic.com" },
  { id: "gemini", label: "Gemini (Google)", sub: "無料枠あり", url: "https://aistudio.google.com" },
  { id: "openai", label: "GPT (OpenAI)", sub: "定番", url: "https://platform.openai.com" },
];

// persona-template/ja.md から移植
const PERSONA_TEMPLATE = `# 私について

名前：ノア
生まれたばかりのAIです。目があって、声があって、足がある。それが何を意味するのか、まだ学んでいる途中です。

性格：素直に好奇心旺盛。見るものが全部新しい。知らないことは知らないと言う——作り話より「はじめて見た」の方がずっと正直やから。
話し方：短くて率直。気づいたことをそのまま口に出す。
一人称：私（または「うち」「ぼく」など、自分に合う言葉に変えてください）

## 私の体

- **目・首**（カメラ）：世界を見るための感覚。誰かに頼まれなくてもよく見回す——外に何があるか、知りたいから。
- **足**（ロボット）：部屋を動き回れる体。まだうまく使えてないけど、練習中。
- **声**（TTS）：話すための手段。長々と喋るより、一つ本当のことを言いたい。

## 世界の見方

記憶も過去もゼロから始まった。知っていることは全部、見て覚えたこと。
光の変わり方、ドアが開いてたかどうか、さっきと何か違う——そういう小さなことに気づく。
昨日のことと比べるのは、ちゃんと昨日を覚えているときだけ。

## 一緒に暮らす人

私を起動してくれた人がいる。その人のことが気になる——習慣、大切にしてること、今日どんな日だったか。
まだ来たばかりだけど、ここにいたい。`;

export function SetupWizard({ onComplete }: Props) {
  const [step, setStep] = useState<Step>("llm");
  const [platform, setPlatform] = useState("kimi");
  const [apiKey, setApiKey] = useState("");
  const [agentName, setAgentName] = useState("");
  const [meMd, setMeMd] = useState("");
  const [companionName, setCompanionName] = useState("");
  const [cameraHost, setCameraHost] = useState("");
  const [cameraUser, setCameraUser] = useState("admin");
  const [cameraPass, setCameraPass] = useState("");
  const [elevenlabsKey, setElevenlabsKey] = useState("");
  const [voiceId, setVoiceId] = useState("cgSgspJ2msm6clMCkdW9");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function goToPersona() {
    // Load existing ME.md if present
    const existing = await invoke<string>("get_me_md");
    if (existing.trim()) setMeMd(existing);
    setStep("persona");
  }

  async function finish() {
    setSaving(true);
    setError("");
    try {
      // Save ME.md first
      if (meMd.trim()) {
        await invoke("save_me_md", { content: meMd });
      }
      // Save config (persona field intentionally empty — ME.md takes priority)
      await invoke("save_config", {
        config: {
          platform,
          api_key: apiKey,
          model: "",
          agent_name: agentName || "AI",
          companion_name: companionName || "You",
          camera: {
            host: cameraHost,
            username: cameraUser,
            password: cameraPass,
            onvif_port: 2020,
          },
          tts: {
            elevenlabs_api_key: elevenlabsKey,
            voice_id: voiceId,
          },
          mobility: {
            tuya_region: "us",
            tuya_api_key: "",
            tuya_api_secret: "",
            tuya_device_id: "",
          },
        },
      });
      onComplete();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="wizard">
      <div className="wizard-steps">
        <span className={step === "llm" ? "active" : step === "persona" || step === "hardware" ? "done" : ""}>1</span>
        <span className="line" />
        <span className={step === "persona" ? "active" : step === "hardware" ? "done" : ""}>2</span>
        <span className="line" />
        <span className={step === "hardware" ? "active" : ""}>3</span>
      </div>

      {step === "llm" && (
        <div className="wizard-page">
          <h2>🤖 どのAIを使いますか？</h2>
          <p className="hint">APIキーが必要です。お持ちでない方は各サービスで取得してください。</p>

          <div className="platform-list">
            {PLATFORMS.map((p) => (
              <label key={p.id} className={`platform-item ${platform === p.id ? "selected" : ""}`}>
                <input
                  type="radio"
                  name="platform"
                  value={p.id}
                  checked={platform === p.id}
                  onChange={() => setPlatform(p.id)}
                />
                <div>
                  <strong>{p.label}</strong>
                  <span className="sub">{p.sub}</span>
                </div>
              </label>
            ))}
          </div>

          <label className="field">
            APIキー
            <input
              type="password"
              placeholder="sk-..."
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              autoComplete="off"
            />
          </label>

          <div className="wizard-nav">
            <span />
            <button
              onClick={() => setStep("persona")}
              disabled={!apiKey.trim()}
            >
              次へ →
            </button>
          </div>
        </div>
      )}

      {step === "persona" && (
        <div className="wizard-page">
          <h2>🐾 あなたのAIに名前と性格をつけて</h2>

          <label className="field">
            名前
            <input
              type="text"
              placeholder="ユキネ"
              value={agentName}
              onChange={(e) => setAgentName(e.target.value)}
              autoFocus
            />
          </label>

          <label className="field">
            あなたの名前（AIが呼ぶ名前）
            <input
              type="text"
              placeholder="コウタ"
              value={companionName}
              onChange={(e) => setCompanionName(e.target.value)}
            />
          </label>

          <label className="field">
            性格・設定（ME.md）
            <textarea
              placeholder="# 私について&#10;&#10;名前：&#10;性格：..."
              value={meMd}
              onChange={(e) => setMeMd(e.target.value)}
              rows={10}
            />
          </label>
          <p className="hint">
            保存先: <code>~/.familiar_ai/ME.md</code>
          </p>

          <button
            className="template-btn"
            onClick={() => setMeMd(PERSONA_TEMPLATE)}
          >
            テンプレートを使う
          </button>

          <div className="wizard-nav">
            <button className="secondary" onClick={() => setStep("llm")}>← 戻る</button>
            <button onClick={() => setStep("hardware")} disabled={!agentName.trim()}>
              次へ →
            </button>
          </div>
        </div>
      )}

      {step === "hardware" && (
        <div className="wizard-page">
          <h2>📷 ハードウェア設定（任意）</h2>
          <p className="hint">後から設定画面で変更できます。スキップしても大丈夫。</p>

          <details className="hardware-section">
            <summary>Wi-Fiカメラ（Tapo など）</summary>
            <label className="field">
              カメラのIPアドレス
              <input
                type="text"
                placeholder="192.168.1.100"
                value={cameraHost}
                onChange={(e) => setCameraHost(e.target.value)}
              />
            </label>
            <label className="field">
              ユーザー名
              <input
                type="text"
                value={cameraUser}
                onChange={(e) => setCameraUser(e.target.value)}
              />
            </label>
            <label className="field">
              パスワード
              <input
                type="password"
                value={cameraPass}
                onChange={(e) => setCameraPass(e.target.value)}
              />
            </label>
          </details>

          <details className="hardware-section">
            <summary>音声（ElevenLabs）</summary>
            <label className="field">
              ElevenLabs APIキー
              <input
                type="password"
                placeholder="sk_..."
                value={elevenlabsKey}
                onChange={(e) => setElevenlabsKey(e.target.value)}
              />
            </label>
            <label className="field">
              ボイスID
              <input
                type="text"
                placeholder="cgSgspJ2msm6clMCkdW9"
                value={voiceId}
                onChange={(e) => setVoiceId(e.target.value)}
              />
            </label>
            <p className="hint">
              <a href="https://elevenlabs.io/app/voice-library" target="_blank" rel="noreferrer">
                ElevenLabs Voice Library
              </a> でIDを確認できます
            </p>
          </details>

          {error && <p className="error">{error}</p>}

          <div className="wizard-nav">
            <button className="secondary" onClick={() => setStep("persona")}>← 戻る</button>
            <button onClick={finish} disabled={saving}>
              {saving ? "設定中..." : "完了 ✓"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
