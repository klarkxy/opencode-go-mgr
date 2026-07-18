import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  compareStableVersions,
  immutableTagDecision,
  normalizeReleaseVersion,
  shouldAdvanceChannel,
  validateComposeVersion,
} from "./release-policy.mjs";

const digest = (character) => `sha256:${character.repeat(64)}`;
const repositoryRoot = fileURLToPath(new URL("../", import.meta.url));

test("stable release channels advance monotonically", () => {
  assert.equal(shouldAdvanceChannel("v1.5.0", "v1.4.9"), true);
  assert.equal(shouldAdvanceChannel("1.5.0", "1.5.0"), false);
  assert.equal(shouldAdvanceChannel("v1.4.9", "v1.5.0"), false);
  assert.equal(shouldAdvanceChannel("v1.5.0", ""), true);
  assert.equal(compareStableVersions("v10.0.0", "v2.99.99"), 1);
  assert.throws(() => shouldAdvanceChannel("v1.5.0-beta.1", "v1.4.2"), /stable semantic version/);
});

test("immutable image tags are created once or retained at the same digest", () => {
  assert.equal(immutableTagDecision({
    tag: "1.5.0",
    candidateDigest: digest("a"),
    existingDigest: "",
  }), "create");
  assert.equal(immutableTagDecision({
    tag: "sha-0123456789ab",
    candidateDigest: digest("a"),
    existingDigest: digest("a"),
  }), "keep");
  assert.throws(() => immutableTagDecision({
    tag: "1.5.0",
    candidateDigest: digest("a"),
    existingDigest: digest("b"),
  }), /Refusing to move immutable container tag/);
});

test("Compose header and default image must match the release version", () => {
  const valid = `# Pull-only Docker Compose example for OCG Manager v1.5.0.\n`
    + `image: \${OCG_IMAGE:-ghcr.io/klarkxy/opencode-go-mgr:1.5.0}\n`;
  assert.equal(validateComposeVersion(valid, "1.5.0"), "1.5.0");
  assert.throws(
    () => validateComposeVersion(valid.replace(/:1\.5\.0}/, ":1.4.2}"), "1.5.0"),
    /Compose version mismatch/,
  );
  assert.throws(
    () => validateComposeVersion(`${valid}${valid}`, "1.5.0"),
    /exactly one/,
  );
  const prerelease = valid.replaceAll("1.5.0", "1.5.0-rc.1");
  assert.equal(validateComposeVersion(prerelease, "v1.5.0-rc.1"), "1.5.0-rc.1");
  assert.equal(normalizeReleaseVersion("v1.5.0-rc.1"), "1.5.0-rc.1");
});

test("manual release runs cannot inherit the production tag path", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  assert.match(workflow, /production: \$\{\{ steps\.matrix\.outputs\.production \}\}/);
  assert.match(
    workflow,
    /if \[\[ "\$GITHUB_EVENT_NAME" == push && "\$GITHUB_REF" == refs\/tags\/v\* \]\]; then/,
  );
  assert.doesNotMatch(workflow, /if:\s*startsWith\(github\.ref, 'refs\/tags\/v'\)/);
  assert.match(
    workflow,
    /name: \$\{\{ needs\.plan\.outputs\.production == 'true' && 'release-signing' \|\| 'release-candidate' \}\}/,
  );
  assert.ok(
    (workflow.match(/needs\.plan\.outputs\.production == 'true'/g) ?? []).length >= 8,
    "every signing, draft, verification, and publication path must use the plan output",
  );
});

test("container publication checks out an exact tag and validates source versions", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );
  assert.match(workflow, /ref: refs\/tags\/\$\{\{ steps\.release\.outputs\.tag \}\}/);
  assert.match(workflow, /git show-ref --verify --quiet "\$expected_ref"/);
  assert.match(workflow, /tag_commit=\$\(git rev-parse "\$expected_ref\^\{commit\}"\)/);
  assert.match(workflow, /node scripts\/release\.mjs --check/);
});

test("release preflight rejects a tag that does not match repository versions", () => {
  const result = spawnSync(
    process.execPath,
    [fileURLToPath(new URL("./release.mjs", import.meta.url)), "--check"],
    {
      cwd: repositoryRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        OCG_RELEASE_TAG: "v9.9.9",
        OCG_REQUIRE_UPDATER_ARTIFACTS: "0",
      },
    },
  );
  assert.notEqual(result.status, 0);
  assert.match(`${result.stdout}\n${result.stderr}`, /Release tag v9\.9\.9 does not match version 1\.5\.0/);
});
