import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";
import type { UpdatePhase } from "../api/tauri.ts";
import { isVersionAtLeast } from "../api/tauri.ts";
import {
  UPDATE_TARGET_STORAGE_KEY,
  clearUpdateTarget,
  decideInstallRequestFailure,
  decideUpdateStatus,
  readUpdateTarget,
  writeUpdateTarget,
} from "./settings-update-state.ts";

class MemoryStorage {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }
}

function status(phase: UpdatePhase, currentVersion: string) {
  return { phase, current_version: currentVersion };
}

test("update recovery classifies completion, observation, failure, and idle states", () => {
  assert.equal(isVersionAtLeast("1.4.1", "1.4.1"), true);
  assert.equal(isVersionAtLeast("v1.5.0", "1.4.1"), true);
  assert.equal(isVersionAtLeast("1.4.0", "1.4.1"), false);
  assert.equal(isVersionAtLeast("1.4.1-beta.1", "1.4.1"), false);

  assert.equal(decideUpdateStatus(status("idle", "1.4.1"), "1.4.1"), "complete");
  assert.equal(decideUpdateStatus(status("failed", "1.5.0"), "1.4.1"), "complete");
  for (const phase of ["checking", "downloading", "installing"] as const) {
    assert.equal(decideUpdateStatus(status(phase, "1.4.0"), "1.4.1"), "busy");
    assert.equal(decideUpdateStatus(status(phase, "1.4.0"), ""), "busy");
  }
  assert.equal(decideUpdateStatus(status("failed", "1.4.0"), "1.4.1"), "failed");
  assert.equal(decideUpdateStatus(status("idle", "1.4.0"), "1.4.1"), "idle");
});

test("update target storage is version-only and degrades to in-memory recovery", () => {
  const storage = new MemoryStorage();
  assert.equal(writeUpdateTarget(storage, " v1.5.0 "), "1.5.0");
  assert.deepEqual([...storage.values], [[UPDATE_TARGET_STORAGE_KEY, "1.5.0"]]);
  assert.equal(readUpdateTarget(storage), "1.5.0");

  clearUpdateTarget(storage);
  assert.equal(readUpdateTarget(storage), "");
  storage.setItem(UPDATE_TARGET_STORAGE_KEY, "release/latest");
  assert.equal(readUpdateTarget(storage), "");
  assert.equal(storage.values.has(UPDATE_TARGET_STORAGE_KEY), false);

  const blockedStorage = {
    getItem(): string | null {
      throw new Error("blocked");
    },
    setItem(): void {
      throw new Error("blocked");
    },
    removeItem(): void {
      throw new Error("blocked");
    },
  };
  assert.equal(writeUpdateTarget(blockedStorage, "1.5.0"), "1.5.0");
  assert.equal(writeUpdateTarget(null, "v1.5.0"), "1.5.0");
  assert.equal(readUpdateTarget(blockedStorage), "");
  assert.doesNotThrow(() => clearUpdateTarget(blockedStorage));
});

test("install request failures preserve the target for restart observation", () => {
  assert.equal(decideInstallRequestFailure(409, true), "observe");
  assert.equal(decideInstallRequestFailure(409, false), "observe");
  assert.equal(decideInstallRequestFailure(null, true), "wait");
  assert.equal(decideInstallRequestFailure(null, false), "fail");
  assert.equal(decideInstallRequestFailure(500, true), "fail");
});

test("update API sends the expected version and exposes polling status", () => {
  const api = readFileSync(new URL("../api/tauri.ts", import.meta.url), "utf8");
  assert.match(api, /install_supported: boolean/);
  assert.match(api, /getUpdateStatus: \(\) => request<UpdateStatus>\("\/settings\/update-status"\)/);
  assert.match(api, /body: jsonBody\(\{ expected_version: expectedVersion \}\)/);
});

test("settings restores and observes updates with bounded, lifecycle-safe polling", () => {
  const settings = readFileSync(new URL("./Settings.vue", import.meta.url), "utf8");
  assert.match(settings, /@positive-click="installAvailableUpdate"/);
  assert.match(settings, /tauriApi\.installUpdate\(result\.latest_version\)/);
  assert.match(settings, /UPDATE_INSTALL_TIMEOUT_MS = 15 \* 60_000/);
  assert.match(settings, /void restoreUpdateState\(\)/);
  assert.match(settings, /readUpdateTarget\(sessionUpdateStorage\(\)\)/);
  assert.match(settings, /isActiveUpdateGeneration\(generation\)/);
  assert.match(settings, /onUnmounted\(\(\) => \{\s*updateDisposed = true;\s*cancelUpdatePolling\(\)/);
  assert.match(settings, /const failureDecision = decideInstallRequestFailure/);
  assert.equal(settings.match(/rememberUpdateTarget\(result\.latest_version\)/g)?.length, 2);
  assert.match(settings, /window\.location\.reload\(\)/);
  assert.match(settings, /updateResult\.release_url/);
  assert.match(settings, /function observeUpdateStatusFailure\(\)[\s\S]*?phase !== "installing"[\s\S]*?updateStatusFallback\("installing"\)/);
  assert.match(settings, /catch \{[\s\S]*?observeUpdateStatusFailure\(\);[\s\S]*?scheduleUpdatePoll\(generation\)/);
  assert.doesNotMatch(settings, /class="update-result" aria-live=/);
  assert.match(settings, /class="sr-only" aria-live="polite" aria-atomic="true"/);

  const finishStart = settings.indexOf("function finishInstalledUpdate");
  const finishEnd = settings.indexOf("function acceptObservedUpdateStatus", finishStart);
  assert.ok(finishStart >= 0 && finishEnd > finishStart);
  const finishSource = settings.slice(finishStart, finishEnd);
  assert.doesNotMatch(finishSource, /checkForUpdate/);
  assert.doesNotMatch(finishSource, /updateDisposed/);
  assert.match(finishSource, /}, 800\)/);
});

test("every translated locale includes the updater interaction copy", () => {
  const messagesDir = new URL("../i18n/messages/", import.meta.url);
  const requiredKeys = [
    "下载并安装",
    "开始升级",
    "正在准备升级…",
    "正在下载升级…",
    "正在安装升级…",
    "正在等待新版本启动…",
    "升级失败",
    "重试升级",
    "已升级到 v{version}",
  ];
  for (const filename of readdirSync(messagesDir).filter((name) => name.endsWith(".ts"))) {
    const source = readFileSync(new URL(filename, messagesDir), "utf8");
    for (const key of requiredKeys) {
      assert.match(source, new RegExp(`"${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"`), `${filename}: ${key}`);
    }
  }
});
