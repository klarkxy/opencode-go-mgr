import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  ARTIFACT_PATHS,
  CARGO_ARGS,
  CARGO_EXAMPLE,
  SCHEMA_RELATIVE_PATH,
  TYPES_RELATIVE_PATH,
  assertArtifactsMatch,
  assertTypesAreContractOnly,
  parseArgs,
  renderTypeScript,
  runContract,
} from "./dashboard-v3-contract.mjs";

const scriptSource = readFileSync(new URL("./dashboard-v3-contract.mjs", import.meta.url), "utf8");

const fixtureSchema = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  title: "DashboardApiV3",
  anyOf: [{ $ref: "#/$defs/ControlRevision" }],
  $defs: {
    ControlRevision: {
      type: "object",
      additionalProperties: false,
      required: ["revision", "processGeneration", "pricingRevision"],
      properties: {
        revision: { type: "integer", minimum: 0 },
        processGeneration: { type: "integer", minimum: 0 },
        pricingRevision: { type: "string" },
      },
    },
  },
};

test("parseArgs accepts exactly one of write or check", () => {
  assert.equal(parseArgs(["--write"]), "write");
  assert.equal(parseArgs(["--check"]), "check");
  assert.throws(() => parseArgs([]), /exactly one/);
  assert.throws(() => parseArgs(["--write", "--check"]), /exactly one/);
  assert.throws(() => parseArgs(["--client"]), /Unknown argument/);
});

test("artifact paths are schema plus types and never an endpoint client", () => {
  assert.deepEqual([...ARTIFACT_PATHS], [SCHEMA_RELATIVE_PATH, TYPES_RELATIVE_PATH]);
  assert.equal(CARGO_EXAMPLE, "export_dashboard_v3_schema");
  assert.deepEqual([...CARGO_ARGS], [
    "run",
    "-p",
    "ocg-core",
    "--example",
    "export_dashboard_v3_schema",
    "--locked",
    "--quiet",
  ]);
  assert.doesNotMatch(scriptSource, /dashboard-v3-client/);
  assert.doesNotMatch(scriptSource, /src\/api\/tauri/);
});

test("generated TypeScript must stay types-only", () => {
  assert.doesNotThrow(() => assertTypesAreContractOnly("export interface ControlRevision { revision: number }\n"));
  assert.throws(
    () => assertTypesAreContractOnly("export async function getContract() { return fetch('/dashboard/api/v3/contract'); }"),
    /types-only/,
  );
  assert.throws(
    () => assertTypesAreContractOnly("export function getContract() { return 1; }"),
    /types-only/,
  );
});

test("renderTypeScript emits interfaces without clients", async () => {
  const ts = await renderTypeScript(fixtureSchema);
  assert.match(ts, /export (interface|type) ControlRevision/);
  assert.match(ts, /processGeneration/);
  assert.doesNotMatch(ts, /\bfetch\s*\(/);
  assert.doesNotMatch(ts, /export async function/);
});

test("check mode detects drift without writing", async () => {
  const writes = [];
  const generatedSchema = `${JSON.stringify(fixtureSchema, null, 2)}\n`;
  await runContract("check", {
    root: fileURLToPath(new URL("../", import.meta.url)),
    exportSchema: () => generatedSchema,
    compileSchema: async () => "export interface ControlRevision { revision: number; }\n",
    readText: (path) => {
      if (path.endsWith("dashboard-api-v3.schema.json")) return generatedSchema;
      return "export interface ControlRevision { revision: number; }\n";
    },
    writeText: (path, contents) => writes.push({ path, contents }),
  });
  assert.equal(writes.length, 0);

  await assert.rejects(
    () => runContract("check", {
      root: fileURLToPath(new URL("../", import.meta.url)),
      exportSchema: () => generatedSchema,
      compileSchema: async () => "export interface Drifted { revision: number; }\n",
      readText: () => "export interface ControlRevision { revision: number; }\n",
      writeText: (path, contents) => writes.push({ path, contents }),
    }),
    /drifted/,
  );
  assert.equal(writes.length, 0);
});

test("write mode only writes the two contract artifacts", async () => {
  const writes = [];
  const generatedSchema = `${JSON.stringify(fixtureSchema, null, 2)}\n`;
  await runContract("write", {
    root: "/tmp/ocg-v3-contract-test",
    exportSchema: () => generatedSchema,
    compileSchema: async () => "export interface ControlRevision { revision: number; }\n",
    writeText: (path, contents) => writes.push({ path: path.replaceAll("\\", "/"), contents }),
  });
  assert.deepEqual(
    writes.map((entry) => entry.path),
    [
      "/tmp/ocg-v3-contract-test/schema/dashboard-api-v3.schema.json",
      "/tmp/ocg-v3-contract-test/src/api/generated/dashboard-v3.ts",
    ],
  );
});

test("assertArtifactsMatch reports both files", () => {
  assert.throws(
    () => assertArtifactsMatch({
      generatedSchema: "a\n",
      generatedTypes: "b\n",
      existingSchema: "A\n",
      existingTypes: "B\n",
    }),
    /schema\/dashboard-api-v3\.schema\.json.*src\/api\/generated\/dashboard-v3\.ts/s,
  );
});
