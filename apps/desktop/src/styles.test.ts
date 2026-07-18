import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const styles = readFileSync(
  join(process.cwd(), "apps/desktop/src/styles.css"),
  "utf8",
);

describe("desktop visual system", () => {
  it("defines the required interaction and responsive rules", () => {
    expect(styles).toContain("--surface:");
    expect(styles).toContain(":focus-visible");
    expect(styles).toContain(".segmented");
    expect(styles).toContain(".overflow-menu");
    expect(styles).toContain("@media (max-width: 760px)");
    expect(styles).toContain("prefers-reduced-motion: reduce");
  });
});
