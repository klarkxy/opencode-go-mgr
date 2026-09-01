import assert from "node:assert/strict";
import test from "node:test";
import type { UpdatePhase } from "../api/dashboard.ts";
import { isVersionAtLeast } from "../utils/version.ts";
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
