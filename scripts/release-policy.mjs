import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const STABLE_VERSION = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const PRERELEASE_IDENTIFIER = "(?:0|[1-9]\\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)";
const RELEASE_VERSION = new RegExp(
  `^v?((0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)(?:-${PRERELEASE_IDENTIFIER}(?:\\.${PRERELEASE_IDENTIFIER})*)?)$`,
);
const DIGEST = /^sha256:[0-9a-f]{64}$/;

export function normalizeReleaseVersion(value, label = "release version") {
  const match = RELEASE_VERSION.exec(value ?? "");
  if (!match) {
    throw new Error(
      `${label} must be semantic version without build metadata, such as v1.5.0 or v1.5.0-rc.1; `
      + `received ${value || "<empty>"}.`,
    );
  }
  return match[1];
}

export function parseStableVersion(value, label = "version") {
  const match = STABLE_VERSION.exec(value ?? "");
  if (!match) {
    throw new Error(`${label} must be a stable semantic version such as v1.5.0; received ${value || "<empty>"}.`);
  }
  return {
    major: BigInt(match[1]),
    minor: BigInt(match[2]),
    patch: BigInt(match[3]),
    version: `${match[1]}.${match[2]}.${match[3]}`,
  };
}

export function isPrereleaseVersion(value, label = "release version") {
  return normalizeReleaseVersion(value, label).includes("-");
}

export function compareStableVersions(left, right) {
  const a = parseStableVersion(left, "candidate version");
  const b = parseStableVersion(right, "current channel version");
  for (const field of ["major", "minor", "patch"]) {
    if (a[field] > b[field]) return 1;
    if (a[field] < b[field]) return -1;
  }
  return 0;
}

export function shouldAdvanceChannel(candidate, current) {
  parseStableVersion(candidate, "candidate version");
  if (!current) return true;
  return compareStableVersions(candidate, current) > 0;
}

export function pairedChannelDecision({ candidate, mainCurrent, browserCurrent }) {
  const candidateVersion = parseStableVersion(candidate, "candidate version").version;
  const mainAdvance = shouldAdvanceChannel(candidateVersion, mainCurrent);
  const browserAdvance = shouldAdvanceChannel(candidateVersion, browserCurrent);
  const mainVersion = mainAdvance
    ? candidateVersion
    : parseStableVersion(mainCurrent, "current main channel version").version;
  const browserVersion = browserAdvance
    ? candidateVersion
    : parseStableVersion(browserCurrent, "current browser channel version").version;

  if (mainVersion !== browserVersion) {
    throw new Error(
      `Refusing to leave paired container channel split: main=${mainVersion}, browser=${browserVersion}, `
      + `candidate=${candidateVersion}.`,
    );
  }

  return { mainAdvance, browserAdvance, version: mainVersion };
}

export function immutableTagDecision({ tag, candidateDigest, existingDigest }) {
  if (!DIGEST.test(candidateDigest ?? "")) {
    throw new Error(`Candidate digest for ${tag} is invalid: ${candidateDigest || "<empty>"}.`);
  }
  if (!existingDigest) return "create";
  if (!DIGEST.test(existingDigest)) {
    throw new Error(`Existing digest for ${tag} is invalid: ${existingDigest}.`);
  }
  if (existingDigest !== candidateDigest) {
    throw new Error(
      `Refusing to move immutable container tag ${tag}: ${existingDigest} != ${candidateDigest}.`,
    );
  }
  return "keep";
}

export function validateComposeVersion(source, expectedVersion) {
  const expected = normalizeReleaseVersion(expectedVersion);
  const headerMatches = [...source.matchAll(/^# Pull-only Docker Compose example for Open Console Gateway v([^\s]+)\.$/gm)];
  const imageMatches = [...source.matchAll(/\$\{OCG_IMAGE:-ghcr\.io\/klarkxy\/opencode-go-mgr:([^}]+)\}/g)];
  const browserImageMatches = [
    ...source.matchAll(/\$\{OCG_BROWSER_IMAGE:-ghcr\.io\/klarkxy\/opencode-go-mgr-browser:([^}]+)\}/g),
  ];
  if (headerMatches.length !== 1 || imageMatches.length !== 1 || browserImageMatches.length !== 1) {
    throw new Error(
      "compose.example.yaml must contain exactly one versioned header, main GHCR image, and browser GHCR image.",
    );
  }
  const headerVersion = headerMatches[0][1];
  const imageVersion = imageMatches[0][1];
  const browserImageVersion = browserImageMatches[0][1];
  if (headerVersion !== expected || imageVersion !== expected || browserImageVersion !== expected) {
    throw new Error(
      `Compose version mismatch: release=${expected}, header=${headerVersion}, `
      + `image=${imageVersion}, browser=${browserImageVersion}.`,
    );
  }
  return expected;
}

function parseOptions(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (!flag?.startsWith("--") || value === undefined) {
      throw new Error("Release policy options must be --name value pairs.");
    }
    options[flag.slice(2)] = value;
  }
  return options;
}

function main(argv) {
  const [command, ...args] = argv;
  const options = parseOptions(args);
  if (command === "should-advance") {
    process.stdout.write(String(shouldAdvanceChannel(options.candidate, options.current)));
    return;
  }
  if (command === "is-prerelease") {
    process.stdout.write(String(isPrereleaseVersion(options.version)));
    return;
  }
  if (command === "paired-channel") {
    process.stdout.write(JSON.stringify(pairedChannelDecision({
      candidate: options.candidate,
      mainCurrent: options["main-current"],
      browserCurrent: options["browser-current"],
    })));
    return;
  }
  if (command === "immutable-tag") {
    process.stdout.write(immutableTagDecision({
      tag: options.tag,
      candidateDigest: options["candidate-digest"],
      existingDigest: options["existing-digest"],
    }));
    return;
  }
  if (command === "validate-compose") {
    const source = readFileSync(resolve(options.file), "utf8");
    process.stdout.write(validateComposeVersion(source, options.version));
    return;
  }
  throw new Error(
    "Usage: release-policy.mjs should-advance|is-prerelease|paired-channel|immutable-tag|validate-compose [options]",
  );
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(error.stack ?? error);
    process.exitCode = 1;
  }
}
