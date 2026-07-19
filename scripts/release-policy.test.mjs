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
const packageVersion = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
).version;

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
  const buildJob = workflow.match(/\n  build:[\s\S]*?\n  draft-release:/)?.[0] ?? "";
  const draftJob = workflow.match(/\n  draft-release:[\s\S]*?\n  verify-release:/)?.[0] ?? "";
  const verifyJob = workflow.match(/\n  verify-release:[\s\S]*?\n  publish-release:/)?.[0] ?? "";
  const publishJob = workflow.match(/\n  publish-release:[\s\S]*$/)?.[0] ?? "";
  assert.match(workflow, /production: \$\{\{ steps\.matrix\.outputs\.production \}\}/);
  assert.match(
    workflow,
    /if \[\[ "\$GITHUB_EVENT_NAME" == push && "\$GITHUB_REF" == refs\/tags\/v\* \]\]; then/,
  );
  assert.doesNotMatch(workflow, /if:\s*startsWith\(github\.ref, 'refs\/tags\/v'\)/);
  assert.doesNotMatch(workflow, /release-signing|release-candidate/);
  assert.doesNotMatch(workflow, /OCG_TAURI_SIGNING|OCG_RELEASE_APPROVAL_ENABLED/);
  assert.match(
    buildJob,
    /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ needs\.plan\.outputs\.production == 'true' && secrets\.TAURI_SIGNING_PRIVATE_KEY \|\| '' \}\}/,
  );
  assert.match(
    buildJob,
    /TAURI_SIGNING_PRIVATE_KEY_PASSWORD: \$\{\{ needs\.plan\.outputs\.production == 'true' && secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD \|\| '' \}\}/,
  );
  assert.doesNotMatch(buildJob, /environment:/);
  assert.match(draftJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(verifyJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(publishJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(publishJob, /needs:\s+- plan\s+- verify-release/);
  assert.doesNotMatch(publishJob, /environment:|always\(\)/);
});

test("the exact draft Release identity flows through verification and publication", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  const draftJob = workflow.match(/\n  draft-release:[\s\S]*?\n  verify-release:/)?.[0] ?? "";
  const verifyJob = workflow.match(/\n  verify-release:[\s\S]*?\n  publish-release:/)?.[0] ?? "";
  const publishJob = workflow.match(/\n  publish-release:[\s\S]*$/)?.[0] ?? "";

  assert.match(draftJob, /release_id: \$\{\{ steps\.release\.outputs\.release_id \}\}/);
  assert.match(draftJob, /- name: Create or update draft release\s+id: release/);
  assert.match(draftJob, /gh release view "\$GITHUB_REF_NAME" --json databaseId,isDraft,tagName/);
  assert.match(verifyJob, /release_id: \$\{\{ steps\.asset_metadata\.outputs\.release_id \}\}/);
  assert.match(verifyJob, /permissions:\s+contents: write/);
  assert.match(verifyJob, /RELEASE_ID: \$\{\{ needs\.draft-release\.outputs\.release_id \}\}/);
  assert.match(verifyJob, /if gh api "repos\/\$GITHUB_REPOSITORY\/releases\/\$RELEASE_ID"/);
  assert.match(verifyJob, /\.id == \$release_id and \.tag_name == \$tag and \.draft == true/);
  assert.match(verifyJob, /\.assets \| length == 15/);
  assert.doesNotMatch(verifyJob, /releases\/tags\//);
  assert.match(publishJob, /RELEASE_ID: \$\{\{ needs\.verify-release\.outputs\.release_id \}\}/);
  assert.match(publishJob, /gh api "repos\/\$GITHUB_REPOSITORY\/releases\/\$RELEASE_ID" > release-metadata\.json/);
  assert.match(publishJob, /--method PATCH "repos\/\$GITHUB_REPOSITORY\/releases\/\$RELEASE_ID"/);
  assert.doesNotMatch(publishJob, /releases\/tags\//);
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
  const output = `${result.stdout}\n${result.stderr}`;
  assert.ok(
    output.includes(`Release tag v9.9.9 does not match version ${packageVersion}`),
    output,
  );
});
