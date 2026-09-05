import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import path from "node:path";
import { after, before, test } from "node:test";
import { pathToFileURL } from "node:url";
import { build } from "vite";
import vue from "@vitejs/plugin-vue";
import { createRenderer, ssrContextKey, type App, type Component } from "vue";

type HostNode = {
  children: HostNode[];
  parent?: HostNode;
  props: Record<string, unknown>;
  text?: string;
  type: string;
};

type CpaApi = Record<string, (...args: unknown[]) => Promise<unknown>>;

let buildDir: string;
let Cpa: Component;
let api: CpaApi;

function cpaHarnessPlugin() {
  const prefix = "\0cpa-component-harness:";
  const modules: Record<string, string> = {
    naive: `
      import { defineComponent, h } from "vue";
      const pass = defineComponent({ inheritAttrs: false, setup(_, { attrs, slots }) {
        return () => h("div", attrs, Object.values(slots).flatMap((slot) => slot?.() ?? []));
      } });
      export const NButton = defineComponent({ inheritAttrs: false, setup(_, { attrs, slots }) { return () => h("button", attrs, slots.default?.()); } });
      export const NAlert = defineComponent({ inheritAttrs: false, setup(_, { attrs, slots }) {
        return () => h("div", attrs, [attrs.title, ...Object.values(slots).flatMap((slot) => slot?.() ?? [])]);
      } });
      export const NCard = pass; export const NEmpty = pass; export const NForm = pass;
      export const NFormItem = pass; export const NInput = pass; export const NSpin = pass; export const NSpace = pass;
      export const NSwitch = pass; export const NTag = pass;
      export const useDialog = () => ({ warning: (options) => options.onPositiveClick?.() });
      export const useMessage = () => ({ error() {}, success() {}, warning() {} });
    `,
    api: `
      export const dashboardV3 = new Proxy({}, { get: (_, key) => (...args) => globalThis.__cpaComponentApi[key](...args) });
    `,
    store: `
      export const useControlPlaneStore = () => ({
        hasTokens: () => true,
        refresh: async () => ({ expectedRevision: 1, processGeneration: 1 }),
        runMutation: async (run) => run({ expectedRevision: 1, processGeneration: 1 }),
      });
    `,
    i18n: `export const t = (key, values = {}) => key.replace(/\\{(\\w+)\\}/g, (_, name) => String(values[name] ?? ""));`,
    errors: `export const dashboardErrorDetail = (error) => error instanceof Error ? error.message : String(error);`,
    clipboard: `
      import { ref } from "vue";
      const copiedTarget = ref("");
      export const useClipboard = () => ({ copiedTarget, copy: async () => {}, cleanup: () => {} });
    `,
  };
  const sources: Record<string, string> = {
    "naive-ui": "naive",
    "../api/dashboard-v3.ts": "api",
    "../stores/controlPlane.ts": "store",
    "../i18n/index.ts": "i18n",
    "../utils/errors.ts": "errors",
    "../utils/format.ts": "clipboard",
  };
  return {
    name: "cpa-component-harness",
    enforce: "pre" as const,
    resolveId(source: string, importer?: string) {
      if (source === "naive-ui") return `${prefix}naive`;
      if (!importer?.replaceAll("\\", "/").includes("/src/views/Cpa.vue")) return null;
      const module = sources[source];
      return module ? `${prefix}${module}` : null;
    },
    load(id: string) {
      if (id.includes("/src/views/Cpa.vue?vue&type=style")) return "";
      return id.startsWith(prefix) ? modules[id.slice(prefix.length)] : null;
    },
  };
}

const renderer = createRenderer<HostNode, HostNode>({
  createComment: (text) => ({ children: [], props: {}, text, type: "comment" }),
  createElement: (type) => ({ children: [], props: {}, type }),
  createText: (text) => ({ children: [], props: {}, text, type: "text" }),
  insert: (child, parent, anchor) => {
    child.parent = parent;
    const index = anchor ? parent.children.indexOf(anchor) : -1;
    if (index >= 0) parent.children.splice(index, 0, child);
    else parent.children.push(child);
  },
  nextSibling: (node) => {
    if (!node.parent) return null;
    const index = node.parent.children.indexOf(node);
    return index >= 0 ? node.parent.children[index + 1] ?? null : null;
  },
  parentNode: (node) => node.parent ?? null,
  patchProp: (node, key, _previous, next) => { node.props[key] = next; },
  remove: (node) => {
    if (!node.parent) return;
    const index = node.parent.children.indexOf(node);
    if (index >= 0) node.parent.children.splice(index, 1);
    node.parent = undefined;
  },
  setElementText: (node, text) => { node.children = []; node.text = text; },
  setText: (node, text) => { node.text = text; },
});

function integration(overrides: Record<string, unknown> = {}) {
  return {
    accountId: null, baseUrl: "http://127.0.0.1:8317", baseUrlReadOnly: false, configured: true,
    currentOperation: null, enabled: true, inferenceKeyConfigured: true, installedVersion: "1.0.0",
    latestVersion: null, managementKeyConfigured: true, modelCount: 1, modelsRefreshedAt: null,
    processGeneration: 1, revision: 1, runtimeOwned: true, runtimeRunning: false, runtimeSupported: true,
    runtimeUnavailableReason: null, updateAvailable: false, ...overrides,
  };
}

function runtime(overrides: Record<string, unknown> = {}) {
  return {
    assetSha256: null, baseUrl: "http://127.0.0.1:8317", currentOperation: null, currentVersion: "1.0.0",
    error: null, installed: true, latestVersion: null, owned: true, phase: "idle", port: 8317,
    previousVersion: null, processGeneration: 1, revision: 1, running: false, supported: true,
    unavailableReason: null, updateAvailable: false, ...overrides,
  };
}

type TestWindow = {
  addEventListener(): void;
  removeEventListener(): void;
  open(url?: unknown): void;
  clearInterval(id?: number): void;
  clearTimeout(id?: number): void;
  setInterval(fn: () => void): number;
  setTimeout(fn: () => void): number;
  __timers: Map<number, () => void>;
  __intervals: Set<number>;
  __opened: string[];
};

async function settle(): Promise<void> {
  for (let index = 0; index < 12; index += 1) await Promise.resolve();
}

function installWindow(): TestWindow {
  const timers = new Map<number, () => void>();
  const intervals = new Set<number>();
  const opened: string[] = [];
  let next = 1;
  const clear = (id?: number) => {
    if (typeof id === "number") {
      timers.delete(id);
      intervals.delete(id);
    }
  };
  const setTimeoutFn = (fn: () => void) => {
    const id = next++;
    timers.set(id, fn);
    return id;
  };
  const setIntervalFn = (fn: () => void) => {
    const id = next++;
    timers.set(id, fn);
    intervals.add(id);
    return id;
  };
  const testWindow: TestWindow = {
    addEventListener() {},
    removeEventListener() {},
    open(url?: unknown) { opened.push(String(url ?? "")); },
    clearInterval: clear,
    clearTimeout: clear,
    setInterval: setIntervalFn,
    setTimeout: setTimeoutFn,
    __timers: timers,
    __intervals: intervals,
    __opened: opened,
  };
  (globalThis as unknown as { window?: TestWindow }).window = testWindow;
  return testWindow;
}

// setTimeout fires once; setInterval re-arms until cleared, matching real timers
// so an overlapping-interval regression stays visible to these tests.
async function fireTimers(testWindow: TestWindow): Promise<void> {
  const entries = [...testWindow.__timers.entries()];
  testWindow.__timers.clear();
  for (const [, fn] of entries) fn();
  for (const [id, fn] of entries) {
    if (testWindow.__intervals.has(id)) testWindow.__timers.set(id, fn);
  }
  await settle();
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void } {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function text(node: HostNode): string {
  return `${node.text ?? ""}${node.children.map(text).join("")}`;
}

function button(root: HostNode, label: string): HostNode {
  const found = root.children.flatMap(function walk(node): HostNode[] {
    return [node, ...node.children.flatMap(walk)];
  }).find((node) => node.type === "button" && text(node).trim() === label);
  assert.ok(found, `button ${label} should render`);
  return found;
}

function oauthComponentApi(overrides: CpaApi): CpaApi {
  return {
    getCpaIntegration: async () => integration(),
    getCpaRuntime: async () => runtime(),
    getCpaAccounts: async () => ({ accounts: [] }),
    getCpaRuntimeKeys: async () => ({ keys: [], processGeneration: 1, revision: 1 }),
    cancelCpaOAuth: async () => ({ revision: 1, processGeneration: 1 }),
    ...overrides,
  };
}

async function mount(componentApi: CpaApi): Promise<{ app: App; root: HostNode; window: TestWindow }> {
  const testWindow = installWindow();
  api = componentApi;
  (globalThis as { __cpaComponentApi?: CpaApi }).__cpaComponentApi = api;
  const root: HostNode = { children: [], props: {}, type: "root" };
  const app = renderer.createApp(Cpa);
  app.provide(ssrContextKey, { modules: new Set<string>() });
  app.mount(root);
  await settle();
  return { app, root, window: testWindow };
}

before(async () => {
  buildDir = await mkdtemp(path.join(process.cwd(), ".ocg-cpa-oauth-"));
  await build({
    configFile: false,
    logLevel: "silent",
    plugins: [cpaHarnessPlugin(), vue()],
    build: {
      emptyOutDir: true,
      lib: {
        entry: path.resolve("src/views/Cpa.vue"),
        fileName: () => "cpa.mjs",
        formats: ["es"],
      },
      outDir: buildDir,
      rollupOptions: { external: ["vue"] },
    },
  });
  Cpa = (await import(pathToFileURL(path.join(buildDir, "cpa.mjs")).href)).default;
});

after(async () => { await rm(buildDir, { force: true, recursive: true }); });

test("OAuth polling is single-flight and schedules the next poll only after completion", async () => {
  const pending = deferred<unknown>();
  let reads = 0;
  const mounted = await mount(oauthComponentApi({
    startCpaOAuth: async () => ({ state: "flow-1", url: null, flow: "browser", revision: 1, processGeneration: 1 }),
    getCpaOAuthStatus: async () => {
      reads += 1;
      return reads === 1 ? pending.promise : { state: "flow-1", status: "pending" };
    },
  }));
  try {
    await (button(mounted.root, "登录 Codex").props.onClick as () => Promise<void>)();
    await settle();
    assert.equal(mounted.window.__timers.size, 1, "the first poll is scheduled");
    await fireTimers(mounted.window);
    assert.equal(reads, 1, "the first status request is in flight");
    await fireTimers(mounted.window);
    assert.equal(reads, 1, "no overlapping request while the previous one is pending");
    pending.resolve({ state: "flow-1", status: "pending" });
    await settle();
    assert.equal(mounted.window.__timers.size, 1, "a non-terminal response schedules the next poll");
    await fireTimers(mounted.window);
    assert.equal(reads, 2, "the next poll runs only after the previous response was applied");
    assert.match(text(mounted.root), /正在等待 CPA 完成授权/);
  } finally { mounted.app.unmount(); }
});

test("a stale terminal response cannot clear a newer OAuth flow", async () => {
  const stale = deferred<unknown>();
  let starts = 0;
  let accountReads = 0;
  const statusStates: string[] = [];
  const mounted = await mount(oauthComponentApi({
    startCpaOAuth: async () => ({ state: `flow-${++starts}`, url: null, flow: "browser", revision: 1, processGeneration: 1 }),
    getCpaOAuthStatus: async (state: unknown) => {
      statusStates.push(String(state));
      return state === "flow-1" ? stale.promise : { state, status: "pending" };
    },
    getCpaAccounts: async () => { accountReads += 1; return { accounts: [] }; },
  }));
  try {
    await (button(mounted.root, "登录 Codex").props.onClick as () => Promise<void>)();
    await settle();
    await fireTimers(mounted.window);
    assert.deepEqual(statusStates, ["flow-1"], "the first flow has a status request in flight");
    await (button(mounted.root, "取消当前授权").props.onClick as () => Promise<void>)();
    await settle();
    assert.doesNotMatch(text(mounted.root), /正在等待 CPA 完成授权/);
    const accountReadsAfterCancel = accountReads;
    await (button(mounted.root, "登录 Codex").props.onClick as () => Promise<void>)();
    await settle();
    assert.equal(starts, 2, "a new flow started");
    assert.match(text(mounted.root), /正在等待 CPA 完成授权/);
    stale.resolve({ state: "flow-1", status: "ok" });
    await settle();
    assert.match(text(mounted.root), /正在等待 CPA 完成授权/, "the stale terminal response leaves the new flow alone");
    assert.equal(accountReads, accountReadsAfterCancel, "the stale success does not refresh accounts");
    await fireTimers(mounted.window);
    assert.deepEqual(statusStates, ["flow-1", "flow-2"], "the new flow keeps polling with its own state");
    assert.match(text(mounted.root), /正在等待 CPA 完成授权/);
  } finally { mounted.app.unmount(); }
});

test("unmounting with a poll in flight cancels the flow and ignores the late response", async () => {
  const stale = deferred<unknown>();
  let cancels = 0;
  let accountReads = 0;
  const mounted = await mount(oauthComponentApi({
    startCpaOAuth: async () => ({ state: "flow-1", url: null, flow: "browser", revision: 1, processGeneration: 1 }),
    getCpaOAuthStatus: async () => stale.promise,
    cancelCpaOAuth: async () => { cancels += 1; return { revision: 1, processGeneration: 1 }; },
    getCpaAccounts: async () => { accountReads += 1; return { accounts: [] }; },
  }));
  await (button(mounted.root, "登录 Codex").props.onClick as () => Promise<void>)();
  await settle();
  await fireTimers(mounted.window);
  const accountReadsBeforeUnmount = accountReads;
  mounted.app.unmount();
  await settle();
  assert.equal(cancels, 1, "leaving the page cancels the active flow");
  stale.resolve({ state: "flow-1", status: "ok" });
  await settle();
  assert.equal(accountReads, accountReadsBeforeUnmount, "the late terminal response is ignored");
});

test("a slow cancel invalidates an in-flight poll synchronously", async () => {
  const stale = deferred<unknown>();
  const cancelRequest = deferred<unknown>();
  let accountReads = 0;
  const mounted = await mount(oauthComponentApi({
    startCpaOAuth: async () => ({ state: "flow-1", url: null, flow: "browser", revision: 1, processGeneration: 1 }),
    getCpaOAuthStatus: async () => stale.promise,
    cancelCpaOAuth: async () => { await cancelRequest.promise; return { revision: 1, processGeneration: 1 }; },
    getCpaAccounts: async () => { accountReads += 1; return { accounts: [] }; },
  }));
  try {
    await (button(mounted.root, "登录 Codex").props.onClick as () => Promise<void>)();
    await settle();
    await fireTimers(mounted.window);
    const accountReadsBeforeCancel = accountReads;
    const cancelClick = (button(mounted.root, "取消当前授权").props.onClick as () => Promise<void>)();
    await settle();
    assert.match(text(mounted.root), /正在等待 CPA 完成授权/, "the flow stays visible while the cancel is pending");
    stale.resolve({ state: "flow-1", status: "ok" });
    await settle();
    assert.equal(accountReads, accountReadsBeforeCancel, "the stale terminal response is ignored during the cancel");
    assert.equal(mounted.window.__timers.size, 0, "the stale response does not re-arm polling");
    assert.match(text(mounted.root), /正在等待 CPA 完成授权/, "the pending cancel still owns the flow state");
    cancelRequest.resolve(undefined);
    await cancelClick;
    await settle();
    assert.doesNotMatch(text(mounted.root), /正在等待 CPA 完成授权/);
    assert.equal(accountReads, accountReadsBeforeCancel, "a cancelled flow never refreshes accounts");
  } finally { mounted.app.unmount(); }
});

test("unmounting while OAuth start is pending never adopts the late session", async () => {
  const startRequest = deferred<unknown>();
  let cancels = 0;
  const mounted = await mount(oauthComponentApi({
    startCpaOAuth: async () => {
      await startRequest.promise;
      return { state: "flow-1", url: "https://example.invalid", flow: "browser", revision: 1, processGeneration: 1 };
    },
    cancelCpaOAuth: async () => { cancels += 1; return { revision: 1, processGeneration: 1 }; },
  }));
  void (button(mounted.root, "登录 Codex").props.onClick as () => Promise<void>)();
  await settle();
  mounted.app.unmount();
  await settle();
  assert.equal(cancels, 0, "no flow was adopted yet, so there is nothing to cancel");
  startRequest.resolve(undefined);
  await settle();
  assert.equal(cancels, 1, "the late-started server session is released best-effort");
  assert.deepEqual(mounted.window.__opened, [], "no popup opens after unmount");
  assert.equal(mounted.window.__timers.size, 0, "no polling is scheduled after unmount");
  assert.doesNotMatch(text(mounted.root), /正在等待 CPA 完成授权/, "the UI does not resurrect the flow");
});
