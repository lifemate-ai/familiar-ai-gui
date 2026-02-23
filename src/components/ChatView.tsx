import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  | { type: "done" }
  | { type: "error"; message: string };

let nextId = 1;

export function ChatView({ onReset }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  const [agentName, setAgentName] = useState("AI");
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
          // Start new assistant message
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
          if (ev.type === "done" || ev.type === "error") {
            if (ev.type === "error") {
              return { ...msg, text: msg.text + `\n[Error: ${ev.message}]`, done: true };
            }
            return { ...msg, done: true };
          }
          return msg;
        });
      });

      if (ev.type === "done" || ev.type === "error") {
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
    }
  }

  async function clearHistory() {
    await invoke("clear_history");
    setMessages([]);
  }

  return (
    <div className="chat">
      <header className="chat-header">
        <span className="agent-name">🐾 {agentName}</span>
        <div className="header-actions">
          <button className="icon-btn" onClick={clearHistory} title="会話をクリア">
            🗑
          </button>
          <button className="icon-btn" onClick={onReset} title="設定">
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
            <div className="bubble">
              {/* Action indicators */}
              {msg.actions.length > 0 && (
                <div className="actions">
                  {msg.actions.map((a, i) => (
                    <span key={i} className="action-tag">{a}</span>
                  ))}
                </div>
              )}
              {/* Message text */}
              <p>{msg.text}</p>
              {/* Thinking indicator */}
              {!msg.done && (
                <span className="thinking-dots">
                  <span>.</span><span>.</span><span>.</span>
                </span>
              )}
            </div>
          </div>
        ))}

        {thinking && messages[messages.length - 1]?.role === "user" && (
          <div className="message assistant">
            <div className="avatar">🐾</div>
            <div className="bubble">
              <span className="thinking-dots">
                <span>.</span><span>.</span><span>.</span>
              </span>
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      <div className="input-area">
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="何か話しかけてみて... (Enter で送信)"
          disabled={thinking}
          rows={2}
        />
        <button onClick={send} disabled={thinking || !input.trim()}>
          送信
        </button>
      </div>
    </div>
  );
}
