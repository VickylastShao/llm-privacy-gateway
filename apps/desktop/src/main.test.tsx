import { describe, expect, it } from "vitest";

describe("desktop security defaults", () => {
  it("documents strict local-only control plane content", () => {
    expect("prompts, provider keys, and pseudonym mappings are never rendered").toContain("never");
  });
});
