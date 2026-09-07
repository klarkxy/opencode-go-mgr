import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (relativePath) => readFileSync(new URL(relativePath, root), "utf8");
const json = (relativePath) => JSON.parse(read(relativePath));

const generatedModelsPlaceholder = "__OCG_MANAGER_GENERATED_MODELS__";

test("Pi package registers the fixed Open Console Gateway provider through Pi's native API-key flow", () => {
  const manifest = json("integrations/pi/package.json");
  const catalog = json("integrations/pi/models.generated.json");

  assert.equal(manifest.name, "ocg-manager-pi");
  assert.deepEqual(manifest.pi.extensions, ["./extensions/ocg-manager.ts"]);
  assert.ok(manifest.keywords.includes("pi-package"));
  assert.equal(manifest.peerDependencies["@earendil-works/pi-ai"], "*");
  assert.equal(manifest.peerDependencies["@earendil-works/pi-coding-agent"], "*");
  assert.equal(catalog.models, generatedModelsPlaceholder);
  assert.equal(
    JSON.stringify(catalog).split(generatedModelsPlaceholder).length - 1,
    1,
    "Pi has one generated model-catalog placeholder",
  );
});

test("DSH bundle inserts one additive companion plugin with the fixed OCG route", () => {
  const manifest = json("integrations/dsh/package.json");

  assert.equal(manifest.name, "ocg-manager-dsh");
  assert.equal(manifest.main, "./index.js");
  assert.equal(manifest.dsh.bundle.patch, "./cordis.patch.yml");
  assert.equal(manifest.peerDependencies["@deepseek-ai/dsh-llm-pi-ai"], ">=0.1.1-rc.2 <0.2.0");
});

test("DSH bundle composes with the installed rc.2 parser without replacing llm-pi-ai", async (t) => {
  const windowsRoot = process.env.APPDATA
    ? join(process.env.APPDATA, "npm", "node_modules")
    : undefined;
  let globalRoot = windowsRoot && existsSync(windowsRoot) ? windowsRoot : undefined;
  try {
    globalRoot ??= execFileSync("npm", ["root", "-g"], { encoding: "utf8" }).trim();
  } catch {}
  if (globalRoot === undefined) {
    t.skip("global npm root is unavailable");
    return;
  }
  const dshRoot = join(globalRoot, "@deepseek-ai", "dsh");
  const appBoot = join(dshRoot, "node_modules", "@deepseek-ai", "dsh-app-boot", "lib", "index.js");
  const basePatch = join(dshRoot, "node_modules", "@deepseek-ai", "dsh-base", "cordis.patch.yml");
  if (!existsSync(appBoot) || !existsSync(basePatch)) {
    t.skip("installed DSH rc.2 composition contract is unavailable");
    return;
  }
  const { composeEntries, loadOverlayPatches } = await import(pathToFileURL(appBoot));
  const warnings = [];
  const rows = composeEntries(
    [
      loadOverlayPatches("dsh-test", basePatch),
      loadOverlayPatches(
        "dsh-test",
        fileURLToPath(new URL("../integrations/dsh/cordis.patch.yml", import.meta.url)),
      ),
    ],
    (warning) => warnings.push(warning),
  );
  const ids = rows.map((row) => row.id);
  assert.ok(ids.includes("llm-pi-ai"));
  assert.ok(ids.includes("ocg-manager-dsh"));
  assert.equal(ids.filter((id) => id === "llm-pi-ai").length, 1);
  assert.deepEqual(warnings, []);
});

test("native package templates contain no literal API key", () => {
  const files = [
    "integrations/pi/package.json",
    "integrations/pi/extensions/ocg-manager.ts",
    "integrations/pi/models.generated.json",
    "integrations/dsh/package.json",
    "integrations/dsh/cordis.patch.yml",
    "integrations/dsh/index.js",
  ];
  const secretLiteral = /(?:sk-[A-Za-z0-9_-]{16,}|Bearer\s+[A-Za-z0-9._-]{16,}|apiKey:\s*["'][^"']+["'])/;

  for (const file of files) {
    assert.doesNotMatch(read(file), secretLiteral, `${join("integrations", file)} contains a literal API key`);
  }
});
