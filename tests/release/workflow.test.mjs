import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = readFileSync(new URL("../../.github/workflows/release.yml", import.meta.url), "utf8");
const codeqlWorkflow = readFileSync(
  new URL("../../.github/workflows/codeql.yml", import.meta.url),
  "utf8",
);

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

test("release attests signed artifacts before creating the draft release", () => {
  assert.match(workflow, /^\s+id-token: write$/m);
  assert.match(workflow, /^\s+attestations: write$/m);

  const signatureStep = workflow.indexOf("name: Verify Authenticode signatures");
  const checksumStep = workflow.indexOf("name: Generate SHA-256 checksums");
  const attestationStep = workflow.indexOf("name: Attest Windows release artifacts");
  const releaseStep = workflow.indexOf("uses: softprops/action-gh-release@");
  assert.ok(
    signatureStep >= 0 &&
      checksumStep > signatureStep &&
      attestationStep > checksumStep &&
      releaseStep > attestationStep,
  );
  assert.match(workflow, /uses: actions\/attest@[0-9a-f]{40}/);
});

test("CodeQL scans both application languages with pinned actions", () => {
  assert.match(codeqlWorkflow, /language: \[javascript-typescript, rust\]/);
  assert.match(codeqlWorkflow, /security-events: write/);
  for (const action of ["init", "autobuild", "analyze"]) {
    assert.match(
      codeqlWorkflow,
      new RegExp(`uses: github/codeql-action/${action}@[0-9a-f]{40}`),
    );
  }
});
