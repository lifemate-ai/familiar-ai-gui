import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SetupWizard } from "./components/SetupWizard";
import { ChatView } from "./components/ChatView";
import "./App.css";

export default function App() {
  const [configured, setConfigured] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("is_configured").then(setConfigured);
  }, []);

  if (configured === null) return <div className="splash">🐾</div>;

  return configured ? (
    <ChatView onReset={() => setConfigured(false)} />
  ) : (
    <SetupWizard onComplete={() => setConfigured(true)} />
  );
}
