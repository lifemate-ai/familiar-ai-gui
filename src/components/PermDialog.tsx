import { invoke } from "@tauri-apps/api/core";

interface Props {
  id: string;
  tool: string;
  detail: string;
  onRespond: () => void;
}

export function PermDialog({ id, tool, detail, onRespond }: Props) {
  async function respond(allowed: boolean) {
    await invoke("respond_permission", { id, allowed });
    onRespond();
  }

  const isDestructive = tool === "bash" || tool === "write_file" || tool === "edit_file";

  return (
    <div className="perm-dialog">
      <div className="perm-dialog-icon">{isDestructive ? "⚠️" : "🔧"}</div>
      <div className="perm-dialog-body">
        <div className="perm-dialog-tool">{tool}</div>
        <div className="perm-dialog-detail">{detail}</div>
      </div>
      <div className="perm-dialog-actions">
        <button className="btn-deny" onClick={() => respond(false)}>
          拒否
        </button>
        <button className="btn-allow" onClick={() => respond(true)}>
          許可
        </button>
      </div>
    </div>
  );
}
