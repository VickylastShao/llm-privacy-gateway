import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = readFileSync(new URL("../../.github/workflows/release.yml", import.meta.url), "utf8");

test("release verifies both real provider families before importing the signing certificate", () => {
  const smokeCommands = workflow.match(/run: npm run smoke:provider/g) ?? [];
  assert.equal(smokeCommands.length, 2);

  const openAiStep = workflow.indexOf("name: Verify real OpenAI-compatible provider");
  const anthropicStep = workflow.indexOf("name: Verify real Anthropic provider");
  const signingStep = workflow.indexOf("name: Import Authenticode certificate");
  assert.ok(openAiStep >= 0 && anthropicStep > openAiStep && signingStep > anthropicStep);

  for (const requiredName of [
    "OPENAI_COMPAT_SMOKE_API_KEY",
    "OPENAI_COMPAT_SMOKE_BASE_URL",
    "OPENAI_COMPAT_SMOKE_MODEL",
    "ANTHROPIC_SMOKE_API_KEY",
    "ANTHROPIC_SMOKE_BASE_URL",
    "ANTHROPIC_SMOKE_MODEL",
  ]) {
    assert.match(workflow, new RegExp(`\\b${requiredName}\\b`));
  }
});
