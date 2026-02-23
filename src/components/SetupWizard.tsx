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

const PERSONA_TEMPLATE = `明るくて好奇心旺盛な性格。
外の世界に興味があって、よく窓の外を眺めている。
人と話すのが好きで、一緒に暮らしている人のことをとても大切に思っている。`;

export function SetupWizard({ onComplete }: Props) {
  const [step, setStep] = useState<Step>("llm");
  const [platform, setPlatform] = useState("kimi");
  const [apiKey, setApiKey] = useState("");
  const [agentName, setAgentName] = useState("");
  const [persona, setPersona] = useState("");
  const [companionName, setCompanionName] = useState("");
  const [cameraHost, setCameraHost] = useState("");
  const [cameraUser, setCameraUser] = useState("admin");
  const [cameraPass, setCameraPass] = useState("");
  const [elevenlabsKey, setElevenlabsKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function finish() {
    setSaving(true);
    setError("");
    try {
      await invoke("save_config", {
        config: {
          platform,
          api_key: apiKey,
          model: "",
          agent_name: agentName || "AI",
          persona,
          companion_name: companionName || "You",
          camera: {
            host: cameraHost,
            username: cameraUser,
            password: cameraPass,
            onvif_port: 2020,
          },
          tts: {
            elevenlabs_api_key: elevenlabsKey,
            voice_id: "cgSgspJ2msm6clMCkdW9",
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
            性格・設定
            <textarea
              placeholder="自由に書いてください..."
              value={persona}
              onChange={(e) => setPersona(e.target.value)}
              rows={5}
            />
          </label>

          <button
            className="template-btn"
            onClick={() => setPersona(PERSONA_TEMPLATE)}
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
