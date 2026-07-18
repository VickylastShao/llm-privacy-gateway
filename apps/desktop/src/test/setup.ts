import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

Object.defineProperty(navigator, "clipboard", {
  configurable: true,
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
  vi.mocked(navigator.clipboard.writeText).mockClear();
});
