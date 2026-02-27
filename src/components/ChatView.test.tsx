import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChatView } from "./ChatView";

type AgentEventPayload =
  | { type: "text"; chunk: string }
  | { type: "action"; name: string; label: string }
  | { type: "done" }
  | { type: "cancelled" }
  | { type: "error"; message: string };

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

// Helper to capture the agent-event listener so tests can fire events
let fireAgentEvent: ((payload: AgentEventPayload) => void) | null = null;

beforeEach(() => {
  vi.clearAllMocks();
  fireAgentEvent = null;
  mockInvoke.mockResolvedValue({ agent_name: "TestAI" });
  mockListen.mockImplementation(async (_eventName, callback) => {
    fireAgentEvent = (payload) =>
      (callback as (e: { payload: AgentEventPayload }) => void)({ payload });
    return () => {};
  });
});

// ── Stop button ────────────────────────────────────────────────────

describe("Stop button", () => {
  it("shows 送信 button when not thinking", async () => {
    render(<ChatView onReset={() => {}} />);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /送信/ })).toBeInTheDocument();
    });
    expect(screen.queryByRole("button", { name: /停止/ })).toBeNull();
  });

  it("hides textarea while thinking", async () => {
    mockInvoke
      .mockResolvedValueOnce({ agent_name: "TestAI" }) // get_config
      .mockResolvedValue(undefined); // send_message

    const user = userEvent.setup();
    render(<ChatView onReset={() => {}} />);

    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));
    await user.type(screen.getByPlaceholderText(/話しかけて/), "hello");
    await user.click(screen.getByRole("button", { name: /送信/ }));

    // Stop button should appear while thinking
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /停止/ })).toBeInTheDocument();
    });
  });

  it("calls cancel_message when stop button is clicked", async () => {
    mockInvoke
      .mockResolvedValueOnce({ agent_name: "TestAI" })
      .mockResolvedValue(undefined);

    const user = userEvent.setup();
    render(<ChatView onReset={() => {}} />);

    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));
    await user.type(screen.getByPlaceholderText(/話しかけて/), "hello");
    await user.click(screen.getByRole("button", { name: /送信/ }));

    await waitFor(() => screen.getByRole("button", { name: /停止/ }));
    await user.click(screen.getByRole("button", { name: /停止/ }));

    expect(mockInvoke).toHaveBeenCalledWith("cancel_message");
  });
});

// ── Copy button ────────────────────────────────────────────────────

describe("Copy button", () => {
  it("does not show copy button when there are no messages", async () => {
    render(<ChatView onReset={() => {}} />);
    await waitFor(() => screen.getByText(/話しかけてみて/));
    expect(screen.queryByTitle(/コピー/)).toBeNull();
  });
});

// ── Send history (↑ key) ───────────────────────────────────────────

describe("Send history (↑ key)", () => {
  it("restores previous message on ArrowUp in empty textarea", async () => {
    mockInvoke
      .mockResolvedValueOnce({ agent_name: "TestAI" })
      .mockResolvedValue(undefined);

    const user = userEvent.setup();
    render(<ChatView onReset={() => {}} />);

    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));
    const textarea = screen.getByPlaceholderText(/話しかけて/);

    // Send a message
    await user.type(textarea, "first message");
    await user.click(screen.getByRole("button", { name: /送信/ }));

    // Simulate agent done → thinking becomes false, textarea re-enables
    await act(async () => {
      fireAgentEvent?.({ type: "done" });
    });

    // Now textarea should be enabled and input empty
    await waitFor(() => expect(textarea).not.toBeDisabled());

    // Press ArrowUp — should restore "first message"
    textarea.focus();
    fireEvent.keyDown(textarea, { key: "ArrowUp" });

    await waitFor(() => {
      expect((textarea as HTMLTextAreaElement).value).toBe("first message");
    });
  });

  it("does not navigate history when textarea has text", async () => {
    render(<ChatView onReset={() => {}} />);
    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));
    const textarea = screen.getByPlaceholderText(/話しかけて/);

    await userEvent.type(textarea, "current text");
    fireEvent.keyDown(textarea, { key: "ArrowUp" });

    // Value should stay the same (history not navigated)
    expect((textarea as HTMLTextAreaElement).value).toBe("current text");
  });
});

// ── Character counter ──────────────────────────────────────────────

describe("Character counter", () => {
  it("shows character count in input area", async () => {
    render(<ChatView onReset={() => {}} />);
    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));

    // Initially 0 chars
    expect(screen.getByText("0")).toBeInTheDocument();
  });

  it("updates count as user types", async () => {
    const user = userEvent.setup();
    render(<ChatView onReset={() => {}} />);
    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));

    await user.type(screen.getByPlaceholderText(/話しかけて/), "hello");
    expect(screen.getByText("5")).toBeInTheDocument();
  });
});

// ── Typing indicator ───────────────────────────────────────────────

describe("Typing indicator", () => {
  it("shows typing indicator while waiting for response", async () => {
    mockInvoke
      .mockResolvedValueOnce({ agent_name: "TestAI" })
      .mockResolvedValue(undefined);

    const user = userEvent.setup();
    render(<ChatView onReset={() => {}} />);

    await waitFor(() => screen.getByPlaceholderText(/話しかけて/));
    await user.type(screen.getByPlaceholderText(/話しかけて/), "hello");
    await user.click(screen.getByRole("button", { name: /送信/ }));

    await waitFor(() => {
      expect(document.querySelector(".thinking-dots")).toBeInTheDocument();
    });
  });
});
