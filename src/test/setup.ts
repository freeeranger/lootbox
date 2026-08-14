import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(cleanup);

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
  invoke: vi.fn(),
  Channel: class {
    onmessage = () => {};
  },
}));

class ResizeObserverMock {
  observe() {}
  disconnect() {}
  unobserve() {}
}

globalThis.ResizeObserver = ResizeObserverMock as typeof ResizeObserver;
