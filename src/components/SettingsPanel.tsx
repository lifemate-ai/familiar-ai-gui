import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Config {
  platform: string;
  api_key: string;
  model: string;
  agent_name: string;
  companion_name: string;
  camera: { host: string; username: string; password: string; onvif_port: number };
  tts: { elevenlabs_api_key: string; voice_id: string };
  mobility: {
    tuya_region: string;
    tuya_api_key: string;
    tuya_api_secret: string;
    tuya_device_id: string;
  };
  coding: { work_dir: string; trust_mode: string; rules: unknown[] };
}

type Tab = "llm" | "persona" | "voice" | "camera" | "coding" | "robot";

const PLATFORMS = [
  { id: "kimi", label: "Kimi K2.5", sub: "おすすめ・コスパ良" },
  { id: "anthropic", label: "Claude (Anthropic)", sub: "高品質" },
  { id: "gemini", label: "Gemini (Google)", sub: "無料枠あり" },
  { id: "openai", label: "GPT (OpenAI)", sub: "定番" },
];

const TABS: { id: Tab; icon: string; label: string }[] = [
  { id: "llm", icon: "🤖", label: "AIモデル" },
  { id: "persona", icon: "🐾", label: "性格・設定" },
  { id: "voice", icon: "🔊", label: "音声" },
  { id: "camera", icon: "📷", label: "カメラ" },
  { id: "coding", icon: "💻", label: "コーディング" },
  { id: "robot", icon: "🦿", label: "ロボット" },
];

interface Props {
  onClose: () => void;
}

export function SettingsPanel({ onClose }: Props) {
  const [tab, setTab] = useState<Tab>("llm");

  // LLM
  const [platform, setPlatform] = useState("kimi");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");

  // Persona
  const [agentName, setAgentName] = useState("");
  const [companionName, setCompanionName] = useState("");
  const [meMd, setMeMd] = useState("");

  // Voice
  const [elevenlabsKey, setElevenlabsKey] = useState("");
  const [voiceId, setVoiceId] = useState("cgSgspJ2msm6clMCkdW9");

  // Camera
  const [cameraHost, setCameraHost] = useState("");
  const [cameraUser, setCameraUser] = useState("admin");
  const [cameraPass, setCameraPass] = useState("");

  // Coding
  const [workDir, setWorkDir] = useState("");
  const [trustMode, setTrustMode] = useState("prompt");

  // Robot
  const [tuyaRegion, setTuyaRegion] = useState("us");
  const [tuyaKey, setTuyaKey] = useState("");
  const [tuyaSecret, setTuyaSecret] = useState("");
  const [tuyaDeviceId, setTuyaDeviceId] = useState("");

  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      invoke<Config>("get_config"),
      invoke<string>("get_me_md"),
    ])
      .then(([cfg, md]) => {
        setPlatform(cfg.platform);
        setApiKey(cfg.api_key);
        setModel(cfg.model);
        setAgentName(cfg.agent_name);
        setCompanionName(cfg.companion_name);
        setElevenlabsKey(cfg.tts.elevenlabs_api_key);
        setVoiceId(cfg.tts.voice_id);
        setCameraHost(cfg.camera.host);
        setCameraUser(cfg.camera.username);
        setCameraPass(cfg.camera.password);
        setWorkDir(cfg.coding.work_dir);
        setTrustMode((cfg.coding.trust_mode as string) || "prompt");
        setTuyaRegion(cfg.mobility.tuya_region || "us");
        setTuyaKey(cfg.mobility.tuya_api_key);
        setTuyaSecret(cfg.mobility.tuya_api_secret);
        setTuyaDeviceId(cfg.mobility.tuya_device_id);
        setMeMd(md);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  async function save() {
    setSaving(true);
    setError("");
    try {
      if (meMd.trim()) {
        await invoke("save_me_md", { content: meMd });
      }
      await invoke("save_config", {
        config: {
          platform,
          api_key: apiKey,
          model,
          agent_name: agentName || "AI",
          companion_name: companionName || "You",
          camera: {
            host: cameraHost,
            username: cameraUser,
            password: cameraPass,
            onvif_port: 2020,
          },
          tts: { elevenlabs_api_key: elevenlabsKey, voice_id: voiceId },
          mobility: {
            tuya_region: tuyaRegion,
            tuya_api_key: tuyaKey,
            tuya_api_secret: tuyaSecret,
            tuya_device_id: tuyaDeviceId,
          },
          coding: { work_dir: workDir, trust_mode: trustMode, rules: [] },
        },
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <div className="settings-loading">読み込み中...</div>;

  return (
    <div className="settings">
      <div className="settings-header">
        <span className="settings-title">⚙️ 設定</span>
        <div className="settings-header-actions">
          {error && (
            <span className="error" style={{ fontSize: "0.8rem" }}>
              {error}
            </span>
          )}
          {saved && (
            <span className="settings-saved">✓ 保存しました</span>
          )}
          <button onClick={save} disabled={saving} style={{ padding: "0.4rem 1.1rem" }}>
            {saving ? "保存中..." : "保存"}
          </button>
          <button className="icon-btn" onClick={onClose} title="閉じる">
            ✕
          </button>
        </div>
      </div>

      <div className="settings-body">
        <nav className="settings-tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`settings-tab${tab === t.id ? " active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              <span className="tab-icon">{t.icon}</span>
              <span className="tab-label">{t.label}</span>
            </button>
          ))}
        </nav>

        <div className="settings-content">
          {tab === "llm" && (
            <section className="settings-section">
              <h3>AIモデル</h3>
              <div className="platform-list">
                {PLATFORMS.map((p) => (
                  <label
                    key={p.id}
                    className={`platform-item ${platform === p.id ? "selected" : ""}`}
                  >
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

              <label className="field">
                モデル名
                <input
                  type="text"
                  placeholder="空欄でデフォルト（例: claude-sonnet-4-6）"
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                />
              </label>
              <p className="hint">
                デフォルト: kimi-k2.5 / claude-haiku-4-5 / gemini-2.5-flash / gpt-4o-mini
              </p>
            </section>
          )}

          {tab === "persona" && (
            <section className="settings-section">
              <h3>性格・設定</h3>
              <div className="settings-row">
                <label className="field">
                  AIの名前
                  <input
                    type="text"
                    placeholder="ユキネ"
                    value={agentName}
                    onChange={(e) => setAgentName(e.target.value)}
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
              </div>

              <label className="field">
                性格・設定（ME.md）
                <textarea
                  value={meMd}
                  onChange={(e) => setMeMd(e.target.value)}
                  rows={18}
                  className="code-textarea"
                  placeholder="# 私について&#10;&#10;名前：&#10;性格：..."
                  spellCheck={false}
                />
              </label>
              <p className="hint">
                保存先:{" "}
                <code>~/.familiar_ai/ME.md</code> · エージェント起動時に読み込まれます
              </p>
            </section>
          )}

          {tab === "voice" && (
            <section className="settings-section">
              <h3>音声（ElevenLabs）</h3>
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
                <a
                  href="https://elevenlabs.io/app/voice-library"
                  target="_blank"
                  rel="noreferrer"
                >
                  ElevenLabs Voice Library
                </a>{" "}
                でIDを確認できます
              </p>
            </section>
          )}

          {tab === "camera" && (
            <section className="settings-section">
              <h3>カメラ（Wi-Fi / Tapo）</h3>
              <label className="field">
                IPアドレス
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
            </section>
          )}

          {tab === "coding" && (
            <section className="settings-section">
              <h3>コーディングエージェント</h3>
              <label className="field">
                作業ディレクトリ（work_dir）
                <input
                  type="text"
                  placeholder="/home/user/myproject"
                  value={workDir}
                  onChange={(e) => setWorkDir(e.target.value)}
                />
              </label>
              <p className="hint">
                設定するとコーディング専用モードになります。プロジェクト情報とワークフロールールが自動注入されます。空欄はホームディレクトリ。
              </p>

              <div className="field" style={{ marginTop: "1rem" }}>
                <span>ツール使用の許可モード</span>
                <div className="trust-list">
                  {[
                    {
                      val: "prompt",
                      label: "🔔 確認する（推奨）",
                      desc: "ファイル書き込み・コマンド実行は毎回確認",
                    },
                    {
                      val: "full",
                      label: "⚡ フルトラスト",
                      desc: "すべてのツールを自動許可（取り扱い注意）",
                    },
                    {
                      val: "custom",
                      label: "🔧 カスタム",
                      desc: "allow/denyルールを手動設定（上級者向け）",
                    },
                  ].map(({ val, label, desc }) => (
                    <label
                      key={val}
                      className={`platform-item ${trustMode === val ? "selected" : ""}`}
                    >
                      <input
                        type="radio"
                        name="trust"
                        value={val}
                        checked={trustMode === val}
                        onChange={() => setTrustMode(val)}
                      />
                      <div>
                        <strong>{label}</strong>
                        <span className="sub">{desc}</span>
                      </div>
                    </label>
                  ))}
                </div>
              </div>
            </section>
          )}

          {tab === "robot" && (
            <section className="settings-section">
              <h3>ロボット（Tuya / 掃除機）</h3>
              <label className="field">
                リージョン
                <input
                  type="text"
                  placeholder="us"
                  value={tuyaRegion}
                  onChange={(e) => setTuyaRegion(e.target.value)}
                />
              </label>
              <label className="field">
                Tuya APIキー（Client ID）
                <input
                  type="password"
                  value={tuyaKey}
                  onChange={(e) => setTuyaKey(e.target.value)}
                />
              </label>
              <label className="field">
                Tuya APIシークレット
                <input
                  type="password"
                  value={tuyaSecret}
                  onChange={(e) => setTuyaSecret(e.target.value)}
                />
              </label>
              <label className="field">
                デバイスID
                <input
                  type="text"
                  value={tuyaDeviceId}
                  onChange={(e) => setTuyaDeviceId(e.target.value)}
                />
              </label>
              <p className="hint">
                <a href="https://iot.tuya.com" target="_blank" rel="noreferrer">
                  Tuya IoT Platform
                </a>{" "}
                で取得できます
              </p>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
