import "@testing-library/jest-dom";

// Mock Tauri APIs (not available in test environment)
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// Mock DOM APIs not implemented in jsdom
window.HTMLElement.prototype.scrollIntoView = vi.fn();

// Mock clipboard API
Object.assign(navigator, {
  clipboard: {
    writeText: vi.fn(() => Promise.resolve()),
  },
});
