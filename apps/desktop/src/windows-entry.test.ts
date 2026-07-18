import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

describe("Windows desktop entry point", () => {
  it("uses the GUI subsystem only for release builds", () => {
    const entry = readFileSync(
      join(process.cwd(), "apps/desktop/src-tauri/src/main.rs"),
      "utf8",
    );
    expect(entry).toContain(
      '#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]',
    );
  });
});
