import {
  createHash,
  createPublicKey,
  verify as verifyEd25519,
} from "node:crypto";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const PLATFORM_ASSETS = Object.freeze({
  "windows-x86_64-nsis": (version) => `ocg-manager_${version}_windows-x64-setup.exe`,
  "linux-x86_64-appimage": (version) => `ocg-manager_${version}_linux-x64.AppImage`,
  "linux-x86_64-deb": (version) => `ocg-manager_${version}_linux-x64.deb`,
  "darwin-x86_64": (version) => `ocg-manager_${version}_macos-universal.app.tar.gz`,
  "darwin-aarch64": (version) => `ocg-manager_${version}_macos-universal.app.tar.gz`,
});
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

function isConfigured(value) {
  return typeof value === "string" && value.trim() !== "";
}

export function resolveUpdaterBuildPlan(env = process.env) {
  const required = env.OCG_REQUIRE_UPDATER_ARTIFACTS === "1";
  const privateKeySource = isConfigured(env.TAURI_SIGNING_PRIVATE_KEY)
    ? "TAURI_SIGNING_PRIVATE_KEY"
    : undefined;
  const publicKey = isConfigured(env.TAURI_UPDATER_PUBLIC_KEY)
    ? env.TAURI_UPDATER_PUBLIC_KEY.trim()
    : undefined;

  if (required && (!privateKeySource || !publicKey)) {
    const missing = [];
    if (!privateKeySource) {
      missing.push("TAURI_SIGNING_PRIVATE_KEY");
    }
    if (!publicKey) missing.push("TAURI_UPDATER_PUBLIC_KEY");
    throw new Error(
      `OCG_REQUIRE_UPDATER_ARTIFACTS=1, but updater signing configuration is missing: ${missing.join(", ")}.`,
    );
  }

  if (privateKeySource && !publicKey) {
    throw new Error(
      "Updater signing is configured, but TAURI_UPDATER_PUBLIC_KEY is missing; refusing to build artifacts the app cannot verify.",
    );
  }

  return {
    enabled: Boolean(privateKeySource && publicKey),
    required,
    privateKeySource,
    publicKey,
  };
}

export function resolveFileSignerEnvironment(env = process.env, pathExists = existsSync) {
  const normalized = { ...env };
  delete normalized.TAURI_SIGNING_PRIVATE_KEY_PATH;
  const privateKey = env.TAURI_SIGNING_PRIVATE_KEY;
  if (isConfigured(privateKey) && pathExists(privateKey)) {
    delete normalized.TAURI_SIGNING_PRIVATE_KEY;
    normalized.TAURI_SIGNING_PRIVATE_KEY_PATH = resolve(privateKey);
  }
  return normalized;
}

function decodeCanonicalBase64(value, label) {
  const encoded = value.trim();
  if (
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)
  ) {
    throw new Error(`${label} is not canonical base64.`);
  }
  const decoded = Buffer.from(encoded, "base64");
  if (decoded.toString("base64") !== encoded) {
    throw new Error(`${label} is not canonical base64.`);
  }
  return decoded;
}

function decodeUtf8Base64(value, label) {
  const decoded = decodeCanonicalBase64(value, label);
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(decoded);
  } catch {
    throw new Error(`${label} does not contain UTF-8 text.`);
  }
}

function parsePublicKey(publicKey) {
  const lines = decodeUtf8Base64(publicKey, "updater public key").trimEnd().split(/\r?\n/);
  if (lines.length !== 2) throw new Error("Updater public key has an invalid Minisign shape.");
  const packet = decodeCanonicalBase64(lines[1], "Minisign public key packet");
  if (
    packet.length !== 42
    || packet[0] !== 0x45
    || ![0x44, 0x64].includes(packet[1])
  ) {
    throw new Error("Updater public key has an unsupported Minisign packet.");
  }
  const key = createPublicKey({
    key: Buffer.concat([ED25519_SPKI_PREFIX, packet.subarray(10)]),
    format: "der",
    type: "spki",
  });
  return { key, keyId: packet.subarray(2, 10) };
}

function parseSignature(signature) {
  const lines = decodeUtf8Base64(signature, "updater signature").trimEnd().split(/\r?\n/);
  if (lines.length !== 4 || !lines[2].startsWith("trusted comment: ")) {
    throw new Error("Updater signature has an invalid Minisign shape.");
  }
  const packet = decodeCanonicalBase64(lines[1], "Minisign signature packet");
  const globalSignature = decodeCanonicalBase64(lines[3], "Minisign global signature");
  if (
    packet.length !== 74
    || globalSignature.length !== 64
    || packet[0] !== 0x45
    || ![0x44, 0x64].includes(packet[1])
  ) {
    throw new Error("Updater signature has an unsupported Minisign packet.");
  }
  return {
    globalSignature,
    isPrehashed: packet[1] === 0x44,
    keyId: packet.subarray(2, 10),
    signature: packet.subarray(10),
    trustedComment: lines[2].slice("trusted comment: ".length),
  };
}

export function verifyUpdaterSignature({ payloadPath, signaturePath, publicKey }) {
  const parsedPublicKey = parsePublicKey(publicKey);
  const parsedSignature = parseSignature(readFileSync(signaturePath, "utf8").trim());
  if (!parsedPublicKey.keyId.equals(parsedSignature.keyId)) {
    throw new Error(`Updater signature key does not match TAURI_UPDATER_PUBLIC_KEY: ${payloadPath}`);
  }

  const payload = readFileSync(payloadPath);
  const signedPayload = parsedSignature.isPrehashed
    ? createHash("blake2b512").update(payload).digest()
    : payload;
  if (!verifyEd25519(null, signedPayload, parsedPublicKey.key, parsedSignature.signature)) {
    throw new Error(`Updater payload signature does not match TAURI_UPDATER_PUBLIC_KEY: ${payloadPath}`);
  }
  const globalPayload = Buffer.concat([
    parsedSignature.signature,
    Buffer.from(parsedSignature.trustedComment, "utf8"),
  ]);
  if (!verifyEd25519(
    null,
    globalPayload,
    parsedPublicKey.key,
    parsedSignature.globalSignature,
  )) {
    throw new Error(`Updater signature metadata does not match TAURI_UPDATER_PUBLIC_KEY: ${payloadPath}`);
  }
}

function parseTag(tag) {
  const match = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(tag);
  if (!match) {
    throw new Error(`Updater manifest requires an immutable version tag such as v1.4.2; received ${tag}.`);
  }
  return tag.slice(1);
}

function parseRepository(repository) {
  const match = /^([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)$/.exec(repository);
  if (!match || [match[1], match[2]].some((part) => part === "." || part === "..")) {
    throw new Error(`Invalid GitHub repository ${repository}; expected owner/name.`);
  }
  return match.slice(1);
}

function requireArtifact(path, label) {
  if (!existsSync(path) || !statSync(path).isFile()) {
    throw new Error(`Missing ${label}: ${path}`);
  }
}

function readSignature(path) {
  requireArtifact(path, "updater signature");
  const signature = readFileSync(path, "utf8").trim();
  if (!signature) throw new Error(`Updater signature is empty: ${path}`);
  return signature;
}

export function buildUpdaterManifest({ releaseDir, tag, repository }) {
  if (!releaseDir) throw new Error("releaseDir is required.");
  const version = parseTag(tag);
  const [owner, name] = parseRepository(repository);
  const platforms = {};

  for (const [platform, assetNameForVersion] of Object.entries(PLATFORM_ASSETS)) {
    const asset = assetNameForVersion(version);
    const payloadPath = resolve(releaseDir, asset);
    requireArtifact(payloadPath, "updater payload");
    const signature = readSignature(`${payloadPath}.sig`);
    const url = `https://github.com/${encodeURIComponent(owner)}/${encodeURIComponent(name)}`
      + `/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(asset)}`;
    platforms[platform] = { signature, url };
  }

  return { version, platforms };
}

export function writeUpdaterManifest(options) {
  const manifest = buildUpdaterManifest(options);
  const output = resolve(options.releaseDir, "latest.json");
  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  return output;
}

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!flag?.startsWith("--") || value === undefined) {
      throw new Error("Usage: generate-updater-manifest.mjs --release-dir <dir> --tag <vX.Y.Z> --repository <owner/name>");
    }
    if (flag === "--release-dir") options.releaseDir = value;
    else if (flag === "--tag") options.tag = value;
    else if (flag === "--repository") options.repository = value;
    else throw new Error(`Unknown argument: ${flag}`);
  }
  return options;
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    const output = writeUpdaterManifest(parseArguments(process.argv.slice(2)));
    console.log(`Updater manifest ready: ${output}`);
  } catch (error) {
    console.error(error.stack ?? error);
    process.exitCode = 1;
  }
}
