import type {
  CpaAccount,
  CpaIntegration,
  CpaOAuthProvider,
  CpaRuntime,
  CpaRuntimeCheck,
  CpaRuntimeKey,
  CpaRuntimePhase,
} from "../api/generated/dashboard-v3.ts";

/**
 * Pure state helpers for the CPA page. The external connection and the managed
 * installation are mutually exclusive modes derived from the integration flag
 * plus the runtime snapshot; every lifecycle control decision is a pure
 * function of (runtime, busy, last update check) so the view stays declarative.
 */

export type CpaRuntimeMode = "external" | "managed" | "unsupported";
export type CpaRuntimeModePreference = Exclude<CpaRuntimeMode, "unsupported"> | null;

/**
 * Pick a usable Overview mode. A fresh supported Desktop defaults to managed
 * install; an already configured external connection defaults to external.
 * A user's explicit choice wins while it remains supported.
 */
/** Confirmed Host support requires a successful runtime snapshot; null is not support. */
export function cpaManagedRuntimeConfirmed(
  integration: Pick<CpaIntegration, "runtimeSupported">,
  runtime: Pick<CpaRuntime, "supported"> | null,
): boolean {
  return integration.runtimeSupported === true && runtime?.supported === true;
}

export function cpaRuntimeMode(
  integration: Pick<CpaIntegration, "configured" | "runtimeOwned" | "runtimeSupported">,
  runtime: Pick<CpaRuntime, "owned" | "supported"> | null,
  preference: CpaRuntimeModePreference = null,
): CpaRuntimeMode {
  const managedSupported = cpaManagedRuntimeConfirmed(integration, runtime);
  if (preference === "managed") {
    if (managedSupported) return "managed";
    if (integration.runtimeSupported !== true) return "unsupported";
    if (runtime === null) return integration.runtimeOwned ? "managed" : "external";
    return "unsupported";
  }
  if (preference === "external") return "external";
  if (integration.runtimeOwned || runtime?.owned) {
    if (managedSupported) return "managed";
    if (runtime === null && integration.runtimeSupported === true) return "managed";
    return "unsupported";
  }
  if (integration.configured) return "external";
  return managedSupported ? "managed" : "external";
}

/** A fresh supported host may install; an installed runtime must be OCG-owned. */
export function cpaRuntimeLifecycleEditable(
  runtime: Pick<CpaRuntime, "supported" | "owned" | "installed"> | null,
): boolean {
  return !!runtime && runtime.supported && (!runtime.installed || runtime.owned);
}

/** The Client Keys section exists only for an owned, installed managed runtime. */
export function cpaClientKeysAvailable(
  runtime: Pick<CpaRuntime, "supported" | "owned" | "installed"> | null,
): boolean {
  return !!runtime && runtime.supported && runtime.owned && runtime.installed;
}

/** Non-terminal phases reported while a lifecycle operation is in flight. */
export const CPA_BUSY_PHASES: readonly CpaRuntimePhase[] = [
  "checking",
  "downloading",
  "installing",
  "starting",
];

export function isCpaPhaseBusy(phase: CpaRuntimePhase): boolean {
  return CPA_BUSY_PHASES.includes(phase);
}

export type CpaRuntimeAction =
  | "install"
  | "start"
  | "stop"
  | "checkUpdate"
  | "update"
  | "rollback"
  | "remove";

export type CpaRuntimeControlState = {
  runtime: CpaRuntime | null;
  /** A lifecycle request is in flight from this client. */
  busy: boolean;
  /** Result of the last explicit check-update, if any. */
  updateCheck: CpaRuntimeCheck | null;
};

const ALL_DISABLED: Record<CpaRuntimeAction, boolean> = {
  install: false,
  start: false,
  stop: false,
  checkUpdate: false,
  update: false,
  rollback: false,
  remove: false,
};

/**
 * Control-availability matrix. Everything is disabled while any operation is
 * in flight (local request or a busy backend phase), on unsupported runtimes,
 * and on runtimes OCG does not own. `update` additionally requires a fresh check that
 * reported an available version, because its `expectedVersion` comes from it.
 */
export function cpaRuntimeControls(state: CpaRuntimeControlState): Record<CpaRuntimeAction, boolean> {
  const { runtime, busy, updateCheck } = state;
  if (!runtime || busy || isCpaPhaseBusy(runtime.phase)) return { ...ALL_DISABLED };
  if (!cpaRuntimeLifecycleEditable(runtime)) return { ...ALL_DISABLED };
  return {
    install: !runtime.installed,
    start: runtime.installed && !runtime.running,
    stop: runtime.installed && runtime.running,
    checkUpdate: true,
    update: runtime.installed && updateCheck?.updateAvailable === true,
    rollback: runtime.installed && !!runtime.previousVersion,
    remove: runtime.installed,
  };
}

/** Client-side bound for the rendered log tail; the backend tail is bounded too. */
export const CPA_LOG_TAIL_LINES = 200;

/** Keep only the last `maxLines` lines of a log payload; blank/empty input renders empty. */
export function cpaLogTail(text: string, maxLines: number = CPA_LOG_TAIL_LINES): string {
  const normalized = text.replace(/\r\n/g, "\n").replace(/\n+$/u, "");
  if (!normalized) return "";
  const lines = normalized.split("\n");
  return lines.slice(Math.max(0, lines.length - maxLines)).join("\n");
}

export type CpaRuntimeKeyPartition = {
  /** OCG-owned routing keys (`protected`); the contract guarantees at most one today. */
  protectedKeys: CpaRuntimeKey[];
  /** Direct-client keys that may be deleted individually. */
  directKeys: CpaRuntimeKey[];
};

/** Protected routing keys never mix with direct-client keys. */
export function partitionCpaRuntimeKeys(keys: readonly CpaRuntimeKey[]): CpaRuntimeKeyPartition {
  const protectedKeys: CpaRuntimeKey[] = [];
  const directKeys: CpaRuntimeKey[] = [];
  for (const key of keys) {
    (key.protected ? protectedKeys : directKeys).push(key);
  }
  return { protectedKeys, directKeys };
}

/** Stable row identity for account lists. */
export function cpaAccountKey(account: Pick<CpaAccount, "name" | "authIndex">): string {
  return `${account.name}:${account.authIndex ?? ""}`;
}

/** Quota payloads are opaque (`any`); render scalars directly and JSON otherwise. */
export function formatCpaQuota(value: unknown): string {
  if (typeof value === "string" || typeof value === "number") return String(value);
  if (value === null || value === undefined) return "—";
  try {
    return JSON.stringify(value) ?? "—";
  } catch {
    return "—";
  }
}

/** CPA OAuth sign-in entry points, in display order. */
export const CPA_OAUTH_PROVIDERS: ReadonlyArray<{ id: CpaOAuthProvider; label: string }> = [
  { id: "codex", label: "Codex" },
  { id: "anthropic", label: "Claude" },
  { id: "antigravity", label: "Antigravity" },
  { id: "kimi", label: "Kimi" },
  { id: "xai", label: "xAI" },
];

const CPA_OAUTH_TERMINAL_STATUSES = ["ok", "completed", "success", "cancelled", "failed", "expired", "error"];
const CPA_OAUTH_SUCCESS_STATUSES = ["ok", "success", "completed"];

/** Polling stops on any terminal OAuth status. */
export function isCpaOAuthTerminalStatus(status: string): boolean {
  return CPA_OAUTH_TERMINAL_STATUSES.includes(status.toLowerCase());
}

/** Only a successful terminal status triggers an account-list refresh. */
export function isCpaOAuthSuccessStatus(status: string): boolean {
  return CPA_OAUTH_SUCCESS_STATUSES.includes(status.toLowerCase());
}
