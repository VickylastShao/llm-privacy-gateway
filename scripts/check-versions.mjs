import { readFile } from "node:fs/promises";

const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const packageLock = JSON.parse(
  await readFile(new URL("../package-lock.json", import.meta.url), "utf8"),
);
const tauriConfig = JSON.parse(
  await readFile(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"),
);
const workspaceCargo = await readFile(new URL("../Cargo.toml", import.meta.url), "utf8");
const desktopCargo = await readFile(
  new URL("../apps/desktop/src-tauri/Cargo.toml", import.meta.url),
  "utf8",
);

function cargoVersion(source, section) {
  const match = source.match(
    new RegExp(`\\[${section.replaceAll(".", "\\.")}\\][\\s\\S]*?^version\\s*=\\s*"([^"]+)"`, "m"),
  );
  if (!match) throw new Error(`Unable to read version from [${section}]`);
  return match[1];
}

const expected = packageJson.version;
const versions = new Map([
  ["package.json", packageJson.version],
  ["package-lock.json", packageLock.version],
  ["package-lock.json root package", packageLock.packages?.[""]?.version],
  ["Cargo.toml workspace", cargoVersion(workspaceCargo, "workspace.package")],
  ["desktop Cargo.toml", cargoVersion(desktopCargo, "package")],
  ["tauri.conf.json", tauriConfig.version],
]);

const mismatches = [...versions].filter(([, version]) => version !== expected);
if (mismatches.length > 0) {
  const details = mismatches.map(([source, version]) => `${source}: ${version ?? "missing"}`).join("\n");
  throw new Error(`Release version mismatch; expected ${expected}:\n${details}`);
}

if (process.env.GITHUB_REF_TYPE === "tag" && process.env.GITHUB_REF_NAME !== `v${expected}`) {
  throw new Error(
    `Release tag ${process.env.GITHUB_REF_NAME ?? "missing"} does not match version v${expected}`,
  );
}

console.log(`Version ${expected} is consistent across release manifests.`);
