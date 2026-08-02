import assert from "node:assert/strict";
import test from "node:test";
import { validateReleaseReadiness } from "../../scripts/check-release-readiness.mjs";

const ready = {
  packageJson: {
    version: "0.1.0",
    repository: { type: "git", url: "https://github.com/acme/llm-privacy-gateway.git" },
  },
  tauriConfig: {
    identifier: "com.acme.llmprivacygateway",
    bundle: { publisher: "Acme" },
  },
  changelog: [
    "## [0.1.0] - 2026-08-02",
    "",
    "[Unreleased]: https://github.com/acme/llm-privacy-gateway/compare/v0.1.0...HEAD",
    "[0.1.0]: https://github.com/acme/llm-privacy-gateway/releases/tag/v0.1.0",
  ].join("\n"),
  security:
    "Use [private security advisories](https://github.com/acme/llm-privacy-gateway/security/advisories/new).",
};

test("accepts a permanent identity, matching links, release date, and private contact", () => {
  assert.deepEqual(validateReleaseReadiness(ready), []);
});

test("reports every temporary public-release field in one run", () => {
  const failures = validateReleaseReadiness({
    packageJson: { version: "0.1.0" },
    tauriConfig: { identifier: "com.local.llmprivacygateway", bundle: {} },
    changelog: [
      "## [0.1.0] - Unreleased",
      "[0.1.0]: https://github.com/OWNER/llm-privacy-gateway/releases/tag/v0.1.0",
    ].join("\n"),
    security: "Report vulnerabilities privately to the maintainer.",
  });

  assert.deepEqual(failures, [
    "tauri identifier is still temporary",
    "bundle publisher is missing",
    "package repository is missing or is not a public GitHub HTTPS URL",
    "changelog still contains placeholder links",
    "changelog release 0.1.0 has no final date",
    "security policy has no concrete private reporting URL",
  ]);
});

test("rejects a Gitee repository because the implemented release workflow is GitHub-only", () => {
  const failures = validateReleaseReadiness({
    ...ready,
    packageJson: {
      ...ready.packageJson,
      repository: "https://gitee.com/acme/llm-privacy-gateway",
    },
  });

  assert.deepEqual(failures, [
    "package repository is missing or is not a public GitHub HTTPS URL",
    "changelog release links do not match the package repository",
  ]);
});

test("rejects a private GitHub advisory URL for a different repository", () => {
  const input = {
    ...ready,
    security: "Use https://github.com/acme/a-different-project/security/advisories/new privately.",
  };

  assert.deepEqual(validateReleaseReadiness(input), [
    "security advisory URL does not match the package repository",
  ]);
});
