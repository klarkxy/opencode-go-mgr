import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceRoot = path.join(repositoryRoot, "docs");
const outputRoot = path.join(repositoryRoot, "target", "pages");
const diagramNames = [
  "local-node",
  "dashboard-v3-mutation",
  "managed-account-lifecycle",
];
const remoteFontBlock = /  <!-- Async font load:[\s\S]*?  <\/noscript>\r?\n/;

await rm(outputRoot, { recursive: true, force: true });
await mkdir(outputRoot, { recursive: true });
await cp(sourceRoot, outputRoot, { recursive: true });

for (const name of diagramNames) {
  const outputPath = path.join(outputRoot, "diagrams", `${name}.html`);
  const source = await readFile(outputPath, "utf8");
  if (!remoteFontBlock.test(source)) {
    throw new Error(`Expected remote font block was not found in ${name}.html`);
  }

  const sanitized = source.replace(remoteFontBlock, "");
  if (/fonts\.(?:googleapis|gstatic)\.com/.test(sanitized)) {
    throw new Error(`Remote font URL remains in ${name}.html`);
  }
  await writeFile(outputPath, sanitized, "utf8");
}

await writeFile(path.join(outputRoot, ".nojekyll"), "", "utf8");
console.log(`Prepared privacy-clean Pages artifact at ${outputRoot}`);
