import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CodeBlock } from "./CodeBlock";
import { PermDialog } from "./PermDialog";
import { SettingsPanel } from "./SettingsPanel";

interface Props {
  onReset: () => void;
}

interface Message {
  id: number;
  role: "user" | "assistant";
  text: string;
  actions: string[];
  done: boolean;
}

type AgentEvent =
  | { type: "text"; chunk: string }
  | { type: "action"; name: string; label: string }
  | { type: "perm_request"; id: string; tool: string; detail: string }
  | { type: "done" }
  | { type: "cancelled" }
  | { type: "error"; message: string };

interface PendingPerm {
  id: string;
  tool: string;
  detail: string;
}

let nextId = 1;

/// Parse text with ```lang\n...\n``` code blocks and render with syntax highlighting.
function MessageContent({ text }: { text: string }) {
  if (!text) return null;
  const parts = text.split(/(```[\w]*\n[\s\S]*?```)/g);
  return (
    <>
      {parts.map((part, i) => {
        const codeMatch = part.match(/^```([\w]*)\n([\s\S]*?)```$/);
        if (codeMatch) {
          const lang = codeMatch[1] || undefined;
          const code = codeMatch[2];
          return <CodeBlock key={i} code={code} language={lang} />;
        }
        return part ? <p key={i} style={{ whiteSpace: "pre-wrap" }}>{part}</p> : null;
      })}
    </>
  );
}

export function ChatView({ onReset: _onReset }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  const [agentName, setAgentName] = useState("AI");
  const [copiedId, setCopiedId] = useState<number | null>(null);
  const [pendingPerm, setPendingPerm] = useState<PendingPerm | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  // Send history: ↑ key restores previous messages
  const [sendHistory, setSendHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number>(-1);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<{ agent_name: string }>("get_config").then((c) =>
      setAgentName(c.agent_name)
    );
  }, []);

  useEffect(() => {
    const unlisten = listen<AgentEvent>("agent-event", (event) => {
      const ev = event.payload;
      setMessages((msgs) => {
        const last = msgs[msgs.length - 1];
        if (!last || last.role !== "assistant" || last.done) {
          const newMsg: Message = {
            id: nextId++,
            role: "assistant",
            text: "",
            actions: [],
            done: false,
          };
          msgs = [...msgs, newMsg];
        }

        return msgs.map((msg, idx) => {
          if (idx !== msgs.length - 1) return msg;
          if (ev.type === "text") {
            return { ...msg, text: msg.text + ev.chunk };
          }
          if (ev.type === "action") {
            return { ...msg, actions: [...msg.actions, ev.label] };
          }
          if (ev.type === "done" || ev.type === "error" || ev.type === "cancelled") {
            if (ev.type === "error") {
              return { ...msg, text: msg.text + `\n[Error: ${ev.message}]`, done: true };
            }
            return { ...msg, done: true };
          }
          return msg;
        });
      });

      if (ev.type === "perm_request") {
        setPendingPerm({ id: ev.id, tool: ev.tool, detail: ev.detail });
        return;
      }

      if (ev.type === "done" || ev.type === "error" || ev.type === "cancelled") {
        setThinking(false);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  async function send() {
    const text = input.trim();
    if (!text || thinking) return;
    setInput("");
    setHistoryIndex(-1);
    setSendHistory((h) => [text, ...h].slice(0, 50));
    setThinking(true);

    const userMsg: Message = {
      id: nextId++,
      role: "user",
      text,
      actions: [],
      done: true,
    };
    setMessages((msgs) => [...msgs, userMsg]);

    try {
      await invoke("send_message", { message: text });
    } catch (e) {
      setMessages((msgs) => [
        ...msgs,
        {
          id: nextId++,
          role: "assistant",
          text: `Error: ${e}`,
          actions: [],
          done: true,
        },
      ]);
      setThinking(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
      return;
    }
    // ↑ key: navigate send history (only when textarea is empty)
    if (e.key === "ArrowUp" && input === "") {
      e.preventDefault();
      const next = historyIndex + 1;
      if (next < sendHistory.length) {
        setHistoryIndex(next);
        setInput(sendHistory[next]);
      }
      return;
    }
    // ↓ key: navigate forward in history
    if (e.key === "ArrowDown" && historyIndex >= 0) {
      e.preventDefault();
      const prev = historyIndex - 1;
      if (prev < 0) {
        setHistoryIndex(-1);
        setInput("");
      } else {
        setHistoryIndex(prev);
        setInput(sendHistory[prev]);
      }
    }
  }

  async function stop() {
    await invoke("cancel_message");
  }

  async function copyMessage(msg: Message) {
    await navigator.clipboard.writeText(msg.text);
    setCopiedId(msg.id);
    setTimeout(() => setCopiedId(null), 1500);
  }

  async function clearHistory() {
    await invoke("clear_history");
    setMessages([]);
    setSendHistory([]);
    setHistoryIndex(-1);
  }

  if (showSettings) {
    return (
      <SettingsPanel
        onClose={() => {
          setShowSettings(false);
          // Refresh agent name after settings save
          invoke<{ agent_name: string }>("get_config").then((c) =>
            setAgentName(c.agent_name)
          );
        }}
      />
    );
  }

  return (
    <div className="chat">
      <header className="chat-header">
        <span className="agent-name">🐾 {agentName}</span>
        <div className="header-actions">
          <button className="icon-btn" onClick={clearHistory} title="会話をクリア">
            🗑
          </button>
          <button
            className="icon-btn"
            onClick={() => setShowSettings(true)}
            title="設定"
          >
            ⚙️
          </button>
        </div>
      </header>

      <div className="messages">
        {messages.length === 0 && (
          <div className="empty-state">
            <p>何か話しかけてみて 👋</p>
          </div>
        )}

        {messages.map((msg) => (
          <div key={msg.id} className={`message ${msg.role}`}>
            {msg.role === "assistant" && (
              <div className="avatar">🐾</div>
            )}
            <div className="bubble-wrap">
              <div className="bubble">
                {msg.actions.length > 0 && (
                  <div className="actions">
                    {msg.actions.map((a, i) => (
                      <span key={i} className="action-tag">{a}</span>
                    ))}
                  </div>
                )}
                <MessageContent text={msg.text} />
                {!msg.done && (
                  <span className="thinking-dots">
                    <span /><span /><span />
                  </span>
                )}
              </div>
              {/* Copy button — visible on hover */}
              {msg.done && msg.text && (
                <button
                  className="copy-btn"
                  onClick={() => copyMessage(msg)}
                  title="コピー"
                >
                  {copiedId === msg.id ? "✓" : "⎘"}
                </button>
              )}
            </div>
          </div>
        ))}

        {thinking && messages[messages.length - 1]?.role === "user" && (
          <div className="message assistant">
            <div className="avatar">🐾</div>
            <div className="bubble-wrap">
              <div className="bubble">
                <span className="thinking-dots">
                  <span /><span /><span />
                </span>
              </div>
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Permission confirmation dialog */}
      {pendingPerm && (
        <PermDialog
          id={pendingPerm.id}
          tool={pendingPerm.tool}
          detail={pendingPerm.detail}
          onRespond={() => setPendingPerm(null)}
        />
      )}

      <div className="input-area">
        <div className="input-wrap">
          <textarea
            value={input}
            onChange={(e) => { setInput(e.target.value); setHistoryIndex(-1); }}
            onKeyDown={handleKeyDown}
            placeholder="何か話しかけてみて... (Enter で送信)"
            disabled={thinking}
            rows={2}
          />
          <span className="char-count">{input.length}</span>
        </div>
        {thinking ? (
          <button className="btn-stop" onClick={stop} title="中断">
            ⏹ 停止
          </button>
        ) : (
          <button onClick={send} disabled={!input.trim()}>
            送信
          </button>
        )}
      </div>
    </div>
  );
}
