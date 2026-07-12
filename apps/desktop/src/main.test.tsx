import { describe, expect, it } from "vitest";

describe("desktop security defaults", () => {
  it("documents strict local-only control plane content", () => {
    expect("prompts, provider keys, and pseudonym mappings are never rendered").toContain("never");
  });

  it("keeps the control-plane page set metadata-only", () => {
    expect("View endpoint, model, status, latency, and category counts only.").not.toContain("provider key");
  });
});
