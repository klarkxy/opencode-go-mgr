import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  buildUpdaterManifest,
  resolveFileSignerEnvironment,
  resolveMacosBundleTargets,
  resolveUpdaterBuildPlan,
  updaterPublicKeyFingerprint,
  verifyUpdaterPublicKeyContinuity,
  verifyUpdaterSignature,
  writeUpdaterManifest,
} from "./generate-updater-manifest.mjs";

const MINISIGN_PUBLIC_KEY = `untrusted comment: minisign public key E7620F1842B4E81F
RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3`;
const MINISIGN_PREHASHED_SIGNATURE = `untrusted comment: signature from minisign secret key
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=
trusted comment: timestamp:1556193335\tfile:test
y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==`;

function updaterEncoded(value) {
  return Buffer.from(value, "utf8").toString("base64");
}

function withReleaseFixture(callback) {
  const directory = mkdtempSync(join(tmpdir(), "ocg-updater-manifest-"));
  const version = "1.4.2";
  const assets = [
    `ocg-manager_${version}_windows-x64-setup.exe`,
    `ocg-manager_${version}_linux-x64.AppImage`,
    `ocg-manager_${version}_linux-x64.deb`,
    `ocg-manager_${version}_macos-universal.app.tar.gz`,
  ];
  for (const [index, asset] of assets.entries()) {
    writeFileSync(join(directory, asset), `payload-${index}`);
    writeFileSync(join(directory, `${asset}.sig`), `signature-${index}\n`);
  }
  try {
    return callback({ assets, directory, version });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test("buildUpdaterManifest emits exact bundle-aware platform keys and immutable tag URLs", () => {
  withReleaseFixture(({ directory }) => {
    const manifest = buildUpdaterManifest({
      releaseDir: directory,
      tag: "v1.4.2",
      repository: "klarkxy/opencode-go-mgr",
    });

    assert.equal(manifest.version, "1.4.2");
    assert.deepEqual(Object.keys(manifest.platforms), [
      "windows-x86_64-nsis",
      "linux-x86_64-appimage",
      "linux-x86_64-deb",
      "darwin-x86_64",
      "darwin-aarch64",
    ]);
    assert.deepEqual(manifest.platforms["windows-x86_64-nsis"], {
      signature: "signature-0",
      url: "https://github.com/klarkxy/opencode-go-mgr/releases/download/v1.4.2/ocg-manager_1.4.2_windows-x64-setup.exe",
    });
    assert.deepEqual(manifest.platforms["linux-x86_64-appimage"], {
      signature: "signature-1",
      url: "https://github.com/klarkxy/opencode-go-mgr/releases/download/v1.4.2/ocg-manager_1.4.2_linux-x64.AppImage",
    });
    assert.deepEqual(manifest.platforms["linux-x86_64-deb"], {
      signature: "signature-2",
      url: "https://github.com/klarkxy/opencode-go-mgr/releases/download/v1.4.2/ocg-manager_1.4.2_linux-x64.deb",
    });
    assert.deepEqual(
      manifest.platforms["darwin-x86_64"],
      manifest.platforms["darwin-aarch64"],
    );
    assert.deepEqual(manifest.platforms["darwin-x86_64"], {
      signature: "signature-3",
      url: "https://github.com/klarkxy/opencode-go-mgr/releases/download/v1.4.2/ocg-manager_1.4.2_macos-universal.app.tar.gz",
    });
  });
});

test("writeUpdaterManifest creates latest.json", () => {
  withReleaseFixture(({ directory }) => {
    const output = writeUpdaterManifest({
      releaseDir: directory,
      tag: "v1.4.2",
      repository: "klarkxy/opencode-go-mgr",
    });
    assert.equal(output, join(directory, "latest.json"));
  });
});

test("missing signature fails closed", () => {
  withReleaseFixture(({ assets, directory }) => {
    rmSync(join(directory, `${assets[2]}.sig`));
    assert.throws(
      () => buildUpdaterManifest({
        releaseDir: directory,
        tag: "v1.4.2",
        repository: "klarkxy/opencode-go-mgr",
      }),
      /Missing updater signature/,
    );
  });
});

test("mutable tags and invalid repositories are rejected", () => {
  withReleaseFixture(({ directory }) => {
    assert.throws(
      () => buildUpdaterManifest({ releaseDir: directory, tag: "latest", repository: "owner/repo" }),
      /immutable version tag/,
    );
    assert.throws(
      () => buildUpdaterManifest({ releaseDir: directory, tag: "v1.4.2-beta.1", repository: "owner/repo" }),
      /immutable version tag/,
    );
    assert.throws(
      () => buildUpdaterManifest({ releaseDir: directory, tag: "v1.4.2", repository: "owner/repo/extra" }),
      /Invalid GitHub repository/,
    );
  });
});

test("updater build plan preserves unsigned local builds", () => {
  assert.deepEqual(resolveUpdaterBuildPlan({}), {
    enabled: false,
    required: false,
    privateKeySource: undefined,
    publicKey: undefined,
  });
  assert.equal(resolveUpdaterBuildPlan({ TAURI_UPDATER_PUBLIC_KEY: "public" }).enabled, false);
});

test("macOS signed builds include the app bundle required by Tauri updater artifacts", () => {
  assert.equal(resolveMacosBundleTargets(false), "dmg");
  assert.equal(resolveMacosBundleTargets(true), "app,dmg");
});

test("updater public-key continuity uses a committed SHA-256 fingerprint", () => {
  const publicKey = "  production-public-key\n";
  const fingerprint = updaterPublicKeyFingerprint(publicKey);
  assert.match(fingerprint, /^[0-9a-f]{64}$/);
  assert.equal(
    verifyUpdaterPublicKeyContinuity({ publicKey, expectedFingerprint: `${fingerprint}\n` }),
    fingerprint,
  );
  assert.throws(
    () => verifyUpdaterPublicKeyContinuity({
      publicKey: "different-key",
      expectedFingerprint: fingerprint,
    }),
    /continuity check failed.*break-glass bootstrap/,
  );
  assert.throws(
    () => verifyUpdaterPublicKeyContinuity({ publicKey, expectedFingerprint: "replace-me" }),
    /fingerprint is missing or invalid/,
  );
});

test("Windows release smoke waits only for bounded installer processes", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  const validator = readFileSync(
    new URL("./validate-windows-release-smoke.ps1", import.meta.url),
    "utf8",
  );
  const smoke = readFileSync(new URL("./smoke-windows-release.ps1", import.meta.url), "utf8");
  assert.doesNotMatch(workflow, /function Invoke-Installer/);
  assert.match(workflow, /\.\/scripts\/smoke-windows-release\.ps1 @parameters/);
  assert.match(validator, /Parser\]::ParseFile/);
  assert.doesNotMatch(validator, /unexpectedly short/);
  assert.match(smoke, /\.WaitForExit\(1000 \* \$TimeoutSeconds\)/);
  assert.match(smoke, /\.WaitForExit\(30000\)/);
  assert.match(smoke, /\.Kill\(\$true\)/);
  assert.doesNotMatch(smoke, /Start-Process \$CandidateInstaller[^\r\n]*-Wait/);
  assert.doesNotMatch(smoke, /Start-Process \$PreviousInstaller[^\r\n]*-Wait/);
  assert.match(smoke, /Wait-UninstallComplete/);
  assert.match(workflow, /\$env:USERPROFILE/);
  assert.doesNotMatch(smoke, /\$env:USERPROFILE\s*=/);
  assert.doesNotMatch(smoke, /\$env:HOME\s*=/);
  assert.match(smoke, /Overwrite update did not preserve the auto-start setting/);
});

test("release workflow keeps reusable quality checks out of the native build matrix", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8",
  );
  const quality = readFileSync(
    new URL("../.github/workflows/quality.yml", import.meta.url),
    "utf8",
  );
  const buildJob = workflow.match(/\n  build:[\s\S]*?\n  draft-release:/)?.[0] ?? "";
  const preflightJob = workflow.match(/\n  preflight:[\s\S]*?\n  build:/)?.[0] ?? "";
  const containerWorkflow = readFileSync(
    new URL("../.github/workflows/container.yml", import.meta.url),
    "utf8",
  );

  assert.match(quality, /\n  pull_request:/);
  assert.match(quality, /pnpm run test/);
  assert.match(quality, /cargo clippy --workspace --all-targets --locked -- -D warnings/);
  assert.match(workflow, /uses: \.\/\.github\/workflows\/quality\.yml/);
  assert.match(
    workflow,
    /if \[\[ "\$GITHUB_EVENT_NAME" == push && "\$GITHUB_REF" == refs\/tags\/v\* \]\]; then/,
  );
  assert.match(workflow, /pnpm run release:check/);
  assert.doesNotMatch(preflightJob, /TAURI_SIGNING_PRIVATE_KEY|OCG_REQUIRE_UPDATER_ARTIFACTS/);
  assert.match(buildJob, /release-signing' \|\| 'release-candidate/);
  assert.match(buildJob, /if: needs\.plan\.outputs\.production == 'true'/);
  assert.match(buildJob, /secrets\.OCG_TAURI_SIGNING_PRIVATE_KEY/);
  assert.doesNotMatch(buildJob, /secrets\.TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(workflow, /matrix: \$\{\{ fromJSON\(needs\.plan\.outputs\.matrix\) \}\}/);
  assert.doesNotMatch(buildJob, /pnpm run (?:test|build:web|design:lint)/);
  assert.doesNotMatch(buildJob, /cargo clippy/);
  assert.match(workflow, /name: assembled-release/);
  assert.match(workflow, /verify-release:/);
  assert.match(workflow, /vars\.OCG_RELEASE_APPROVAL_ENABLED == 'true'/);
  assert.match(workflow, /environment:\s+name: release/);
  assert.match(workflow, /Refusing to publish: draft assets changed after verification/);
  assert.match(workflow, /group: release-moving-channels\s+queue: max/);
  assert.match(workflow, /release-policy\.mjs should-advance/);
  assert.match(quality, /windows-tauri:/);
  assert.match(quality, /cargo test -p ocg-manager --lib --locked/);
  assert.match(containerWorkflow, /push-by-digest=true/);
  assert.match(containerWorkflow, /release-policy\.mjs immutable-tag/);
  assert.match(containerWorkflow, /group: ghcr-moving-channels\s+queue: max/);
});

test("updater build plan accepts TAURI_SIGNING_PRIVATE_KEY content or path with a public key", () => {
  assert.equal(resolveUpdaterBuildPlan({
    TAURI_SIGNING_PRIVATE_KEY: "private",
    TAURI_UPDATER_PUBLIC_KEY: " public ",
  }).enabled, true);
  assert.equal(resolveUpdaterBuildPlan({
    TAURI_SIGNING_PRIVATE_KEY: "private.key",
    TAURI_UPDATER_PUBLIC_KEY: "public",
  }).privateKeySource, "TAURI_SIGNING_PRIVATE_KEY");
});

test("file signer normalizes a direct private-key path and ignores external path variables", () => {
  const pathEnvironment = resolveFileSignerEnvironment({
    TAURI_SIGNING_PRIVATE_KEY: "private.key",
    TAURI_SIGNING_PRIVATE_KEY_PATH: "unsupported.key",
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "password",
  }, () => true);
  assert.equal(pathEnvironment.TAURI_SIGNING_PRIVATE_KEY, undefined);
  assert.equal(pathEnvironment.TAURI_SIGNING_PRIVATE_KEY_PATH, join(process.cwd(), "private.key"));
  assert.equal(pathEnvironment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD, "password");

  const contentEnvironment = resolveFileSignerEnvironment({
    TAURI_SIGNING_PRIVATE_KEY: "base64-content",
    TAURI_SIGNING_PRIVATE_KEY_PATH: "unsupported.key",
  }, () => false);
  assert.equal(contentEnvironment.TAURI_SIGNING_PRIVATE_KEY, "base64-content");
  assert.equal(contentEnvironment.TAURI_SIGNING_PRIVATE_KEY_PATH, undefined);
});

test("updater signature verification accepts a valid prehashed Minisign fixture", () => {
  const directory = mkdtempSync(join(tmpdir(), "ocg-updater-signature-"));
  const payloadPath = join(directory, "payload.bin");
  const signaturePath = `${payloadPath}.sig`;
  try {
    writeFileSync(payloadPath, "test");
    writeFileSync(signaturePath, updaterEncoded(MINISIGN_PREHASHED_SIGNATURE));
    assert.doesNotThrow(() => verifyUpdaterSignature({
      payloadPath,
      signaturePath,
      publicKey: updaterEncoded(MINISIGN_PUBLIC_KEY),
    }));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("updater signature verification rejects tampered payloads and mismatched public keys", () => {
  const directory = mkdtempSync(join(tmpdir(), "ocg-updater-signature-negative-"));
  const payloadPath = join(directory, "payload.bin");
  const signaturePath = `${payloadPath}.sig`;
  try {
    writeFileSync(signaturePath, updaterEncoded(MINISIGN_PREHASHED_SIGNATURE));
    writeFileSync(payloadPath, "Test");
    assert.throws(
      () => verifyUpdaterSignature({
        payloadPath,
        signaturePath,
        publicKey: updaterEncoded(MINISIGN_PUBLIC_KEY),
      }),
      /payload signature does not match/,
    );

    writeFileSync(payloadPath, "test");
    const [comment, encodedPacket] = MINISIGN_PUBLIC_KEY.split("\n");
    const mismatchedPacket = Buffer.from(encodedPacket, "base64");
    mismatchedPacket[2] ^= 0xff;
    const mismatchedPublicKey = updaterEncoded(
      `${comment}\n${mismatchedPacket.toString("base64")}`,
    );
    assert.throws(
      () => verifyUpdaterSignature({ payloadPath, signaturePath, publicKey: mismatchedPublicKey }),
      /signature key does not match/,
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("required updater artifacts fail before build when either key is missing", () => {
  assert.throws(
    () => resolveUpdaterBuildPlan({ OCG_REQUIRE_UPDATER_ARTIFACTS: "1" }),
    /TAURI_SIGNING_PRIVATE_KEY.*TAURI_UPDATER_PUBLIC_KEY/,
  );
  assert.throws(
    () => resolveUpdaterBuildPlan({
      OCG_REQUIRE_UPDATER_ARTIFACTS: "1",
      TAURI_SIGNING_PRIVATE_KEY: "private",
    }),
    /TAURI_UPDATER_PUBLIC_KEY/,
  );
});

test("a signing key without a public verification key is always rejected", () => {
  assert.throws(
    () => resolveUpdaterBuildPlan({ TAURI_SIGNING_PRIVATE_KEY: "private" }),
    /TAURI_UPDATER_PUBLIC_KEY is missing/,
  );
});
