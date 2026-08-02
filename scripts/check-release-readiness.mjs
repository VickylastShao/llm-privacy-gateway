import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

function repositoryUrl(repository) {
  const value = typeof repository === "string" ? repository : repository?.url;
  return typeof value === "string"
    ? value.replace(/^git\+/, "").replace(/\.git$/, "").replace(/\/$/, "")
    : "";
}

export function validateReleaseReadiness({ packageJson, tauriConfig, changelog, security }) {
  const failures = [];
  const identifier = String(tauriConfig.identifier ?? "");
  if (
    identifier.length === 0 ||
    identifier === "com.local.llmprivacygateway" ||
    /(^|\.)(local|example)(\.|$)/i.test(identifier)
  ) {
    failures.push("tauri identifier is still temporary");
  }

  const publisher = String(tauriConfig.bundle?.publisher ?? "").trim();
  if (publisher.length === 0) failures.push("bundle publisher is missing");

  const repository = repositoryUrl(packageJson.repository);
  const isPublicGitHubRepository = /^https:\/\/github\.com\/[^/]+\/[^/]+$/i.test(repository);
  if (!isPublicGitHubRepository) {
    failures.push("package repository is missing or is not a public GitHub HTTPS URL");
  }

  if (/github\.com\/OWNER\/|<[^>]+>|\{\{[^}]+\}\}/i.test(changelog)) {
    failures.push("changelog still contains placeholder links");
  } else if (repository && !changelog.includes(repository)) {
    failures.push("changelog release links do not match the package repository");
  }

  const version = String(packageJson.version ?? "");
  const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (!new RegExp(`^## \\[${escapedVersion}\\] - \\d{4}-\\d{2}-\\d{2}$`, "m").test(changelog)) {
    failures.push(`changelog release ${version} has no final date`);
  }

  const hasPrivateContact =
    /mailto:[^\s)]+@[^\s)]+/i.test(security) ||
    /https:\/\/github\.com\/[^/]+\/[^/]+\/security\/advisories\/new/i.test(security) ||
    /https:\/\/[^\s)]+\/(security|vulnerability|disclosure)[^\s)]*/i.test(security);
  if (!hasPrivateContact) failures.push("security policy has no concrete private reporting URL");

  const advisoryUrls = [
    ...security.matchAll(
      /https:\/\/github\.com\/[^/\s)]+\/[^/\s)]+\/security\/advisories\/new/gi,
    ),
  ].map(([url]) => url.replace(/\/$/, ""));
  if (
    isPublicGitHubRepository &&
    advisoryUrls.length > 0 &&
    !advisoryUrls.some((url) => url === `${repository}/security/advisories/new`)
  ) {
    failures.push("security advisory URL does not match the package repository");
  }

  return failures;
}

async function readCurrentRepository() {
  return {
    packageJson: JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8")),
    tauriConfig: JSON.parse(
      await readFile(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"),
    ),
    changelog: await readFile(new URL("../CHANGELOG.md", import.meta.url), "utf8"),
    security: await readFile(new URL("../SECURITY.md", import.meta.url), "utf8"),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const failures = validateReleaseReadiness(await readCurrentRepository());
  if (failures.length > 0) {
    console.error("Public release preflight failed:");
    for (const failure of failures) console.error(`- ${failure}`);
    process.exitCode = 1;
  } else {
    console.log("Public release identity, repository links, date, and private contact are ready.");
  }
}
