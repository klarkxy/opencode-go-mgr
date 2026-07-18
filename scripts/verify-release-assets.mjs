import { createHash } from "node:crypto";
import {
  createReadStream,
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
} from "node:fs";
import { resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { fileURLToPath } from "node:url";

import {
  buildUpdaterManifest,
  verifyUpdaterSignature,
} from "./generate-updater-manifest.mjs";

function fail(message) {
  throw new Error(message);
}

function expectedAssets(version) {
  return [
    `ocg-manager_${version}_windows-x64-setup.exe`,
    `ocg-manager_${version}_windows-x64-setup.exe.sig`,
    `ocg-manager-cli_${version}_windows-x64.zip`,
    `ocg-manager_${version}_macos-universal.dmg`,
    `ocg-manager_${version}_macos-universal.app.tar.gz`,
    `ocg-manager_${version}_macos-universal.app.tar.gz.sig`,
    `ocg-manager-cli_${version}_macos-universal.tar.gz`,
    `ocg-manager_${version}_linux-x64.AppImage`,
    `ocg-manager_${version}_linux-x64.AppImage.sig`,
    `ocg-manager_${version}_linux-x64.deb`,
    `ocg-manager_${version}_linux-x64.deb.sig`,
    `ocg-manager-cli_${version}_linux-x64.tar.gz`,
    "compose.example.yaml",
    "latest.json",
    "SHA256SUMS",
  ].sort();
}

export function parseChecksums(contents) {
  const checksums = new Map();
  for (const line of contents.trimEnd().split(/\r?\n/)) {
    const match = /^([0-9a-f]{64})  ([^/\\]+)$/.exec(line);
    if (!match) fail(`Invalid SHA256SUMS line: ${line}`);
    if (checksums.has(match[2])) fail(`Duplicate SHA256SUMS entry: ${match[2]}`);
    checksums.set(match[2], match[1]);
  }
  return checksums;
}

async function sha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

function requireFile(path, label) {
  if (!existsSync(path) || !statSync(path).isFile()) fail(`Missing ${label}: ${path}`);
}

function parseMetadata(path) {
  if (!path) return undefined;
  const metadata = JSON.parse(readFileSync(path, "utf8"));
  if (!Array.isArray(metadata)) fail("Release asset metadata must be an array.");
  const assets = new Map();
  for (const asset of metadata) {
    if (typeof asset?.name !== "string" || typeof asset?.digest !== "string") {
      fail("Release asset metadata requires string name and digest fields.");
    }
    if (assets.has(asset.name)) fail(`Duplicate release asset metadata: ${asset.name}`);
    assets.set(asset.name, asset.digest);
  }
  return assets;
}

export async function verifyReleaseAssets({
  releaseDir,
  tag,
  repository,
  assetMetadataPath,
  publicKey,
}) {
  const directory = resolve(releaseDir);
  const expectedManifest = buildUpdaterManifest({ releaseDir: directory, tag, repository });
  const expectedNames = expectedAssets(expectedManifest.version);
  const entries = readdirSync(directory, { withFileTypes: true });
  const unexpectedEntry = entries.find((entry) => !entry.isFile());
  if (unexpectedEntry) fail(`Release directory contains a non-file entry: ${unexpectedEntry.name}`);
  const actualNames = entries.map((entry) => entry.name).sort();
  if (!isDeepStrictEqual(actualNames, expectedNames)) {
    fail(`Release asset set mismatch. Expected ${expectedNames.join(", ")}; found ${actualNames.join(", ")}.`);
  }

  const manifestPath = resolve(directory, "latest.json");
  const actualManifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (!isDeepStrictEqual(actualManifest, expectedManifest)) {
    fail("latest.json does not match the immutable updater manifest derived from release assets.");
  }

  const checksumPath = resolve(directory, "SHA256SUMS");
  const checksums = parseChecksums(readFileSync(checksumPath, "utf8"));
  const checksumNames = [...checksums.keys()].sort();
  const expectedChecksumNames = expectedNames.filter((name) => name !== "SHA256SUMS");
  if (!isDeepStrictEqual(checksumNames, expectedChecksumNames)) {
    fail("SHA256SUMS does not cover the exact release asset set.");
  }

  const computed = new Map();
  for (const name of expectedNames) {
    const path = resolve(directory, name);
    requireFile(path, "release asset");
    const digest = await sha256(path);
    computed.set(name, digest);
    if (name !== "SHA256SUMS" && checksums.get(name) !== digest) {
      fail(`SHA-256 mismatch for ${name}.`);
    }
  }

  if (publicKey) {
    const signedPayloads = [
      `ocg-manager_${expectedManifest.version}_windows-x64-setup.exe`,
      `ocg-manager_${expectedManifest.version}_macos-universal.app.tar.gz`,
      `ocg-manager_${expectedManifest.version}_linux-x64.AppImage`,
      `ocg-manager_${expectedManifest.version}_linux-x64.deb`,
    ];
    for (const name of signedPayloads) {
      const payloadPath = resolve(directory, name);
      verifyUpdaterSignature({
        payloadPath,
        signaturePath: `${payloadPath}.sig`,
        publicKey,
      });
    }
  }

  const metadata = parseMetadata(assetMetadataPath);
  if (metadata) {
    if (!isDeepStrictEqual([...metadata.keys()].sort(), expectedNames)) {
      fail("GitHub Release asset metadata does not match the assembled release asset set.");
    }
    for (const name of expectedNames) {
      const expectedDigest = `sha256:${computed.get(name)}`;
      if (metadata.get(name) !== expectedDigest) {
        fail(`GitHub Release digest mismatch for ${name}.`);
      }
    }
  }

  return {
    assetCount: expectedNames.length,
    version: expectedManifest.version,
  };
}

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!flag?.startsWith("--") || value === undefined) {
      fail("Usage: verify-release-assets.mjs --release-dir <dir> --tag <vX.Y.Z> --repository <owner/name> [--asset-metadata <json>]");
    }
    if (flag === "--release-dir") options.releaseDir = value;
    else if (flag === "--tag") options.tag = value;
    else if (flag === "--repository") options.repository = value;
    else if (flag === "--asset-metadata") options.assetMetadataPath = value;
    else fail(`Unknown argument: ${flag}`);
  }
  options.publicKey = process.env.TAURI_UPDATER_PUBLIC_KEY?.trim() || undefined;
  if (!options.publicKey) {
    fail("TAURI_UPDATER_PUBLIC_KEY is required when verifying a GitHub Release.");
  }
  return options;
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    const result = await verifyReleaseAssets(parseArguments(process.argv.slice(2)));
    console.log(`Verified ${result.assetCount} release assets for v${result.version}.`);
  } catch (error) {
    console.error(error.stack ?? error);
    process.exitCode = 1;
  }
}
