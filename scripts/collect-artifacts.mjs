import { basename, join } from "node:path";
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";

const releaseDir = "release";
const nsisDir = "target/release/bundle/nsis";

const files = [
  ["target/release/ocg-manager.exe", "ocg-manager.exe"],
  ["target/release/ocg-manager-cli.exe", "ocg-manager-cli.exe"],
];

const installers = existsSync(nsisDir)
  ? readdirSync(nsisDir)
      .filter((name) => name.endsWith("-setup.exe"))
      .map((name) => join(nsisDir, name))
      .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)
  : [];

if (installers[0]) {
  files.push([installers[0], basename(installers[0])]);
}

const missing = files.filter(([source]) => !existsSync(source)).map(([source]) => source);
const dashboardDist = existsSync("dist")
  ? "dist"
  : existsSync("target/release/dist")
    ? "target/release/dist"
    : null;

if (files.length < 3 || !dashboardDist || missing.length > 0) {
  console.error("Missing release artifacts. Run `pnpm run build:all` first.");
  for (const file of missing) {
    console.error(`  ${file}`);
  }
  if (!dashboardDist) {
    console.error("  target/release/dist or dist");
  }
  process.exit(1);
}

rmSync(releaseDir, { recursive: true, force: true });
mkdirSync(releaseDir, { recursive: true });

for (const [source, target] of files) {
  const destination = join(releaseDir, target);
  cpSync(source, destination);
  console.log(`${destination}`);
}

cpSync(dashboardDist, join(releaseDir, "dist"), { recursive: true });
console.log(`${join(releaseDir, "dist")}`);
