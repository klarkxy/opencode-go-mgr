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
  open(): void;
  clearInterval(id?: number): void;
  clearTimeout(id?: number): void;
  setInterval(fn: () => void): number;
  setTimeout(fn: () => void): number;
  __timers: Map<number, () => void>;
};

async function settle(): Promise<void> {
  for (let index = 0; index < 12; index += 1) await Promise.resolve();
}

function installWindow(): TestWindow {
  const timers = new Map<number, () => void>();
  let next = 1;
  const clear = (id?: number) => {
    if (typeof id === "number") timers.delete(id);
  };
  const set = (fn: () => void) => {
    const id = next++;
    timers.set(id, fn);
    return id;
  };
  const testWindow: TestWindow = {
    addEventListener() {},
    removeEventListener() {},
    open() {},
    clearInterval: clear,
    clearTimeout: clear,
    setInterval: set,
    setTimeout: set,
    __timers: timers,
  };
  (globalThis as unknown as { window?: TestWindow }).window = testWindow;
  return testWindow;
}

async function fireTimers(testWindow: TestWindow): Promise<void> {
  const fns = [...testWindow.__timers.values()];
  testWindow.__timers.clear();
  for (const fn of fns) fn();
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
  buildDir = await mkdtemp(path.join(process.cwd(), ".ocg-cpa-component-"));
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

test("a synchronous lifecycle success immediately refreshes integration, accounts, and keys", async () => {
  let running = false;
  const calls = { accounts: 0, integration: 0, keys: 0 };
  const mounted = await mount({
    getCpaIntegration: async () => { calls.integration += 1; return integration({ runtimeRunning: running }); },
    getCpaRuntime: async () => runtime({ running }),
    getCpaAccounts: async () => { calls.accounts += 1; return { accounts: [] }; },
    getCpaRuntimeKeys: async () => { calls.keys += 1; return { keys: [], processGeneration: 1, revision: 1 }; },
    startCpaRuntime: async () => { running = true; return runtime({ running: true }); },
  });
  calls.accounts = calls.integration = calls.keys = 0;
  const start = button(mounted.root, "启动");
  await (start.props.onClick as () => Promise<void>)();
  await settle();
  assert.equal(calls.integration, 1);
  assert.equal(calls.accounts, 1);
  assert.equal(calls.keys, 1);
  assert.match(text(mounted.root), /运行中/);
  mounted.app.unmount();
});

test("a successful client Key creation keeps its one-time secret visible when list refresh fails", async () => {
  let keyReads = 0;
  const mounted = await mount({
    getCpaIntegration: async () => integration({ runtimeRunning: true }),
    getCpaRuntime: async () => runtime({ running: true }),
    getCpaAccounts: async () => ({ accounts: [] }),
    getCpaRuntimeKeys: async () => {
      keyReads += 1;
      if (keyReads > 1) throw new Error("list refresh failed");
      return { keys: [], processGeneration: 1, revision: 1 };
    },
    createCpaRuntimeKey: async () => ({ fingerprint: "fp-new", hint: "sk-…new", processGeneration: 1, revision: 2, secret: "sk-one-time-secret" }),
  });
  await (button(mounted.root, "添加客户端 Key").props.onClick as () => Promise<void>)();
  await settle();
  assert.equal(keyReads, 2);
  assert.match(text(mounted.root), /sk-one-time-secret/);
  mounted.app.unmount();
});

test("a runtime fetch failure is a visible recoverable error and not confirmed managed support", async () => {
  const mounted = await mount({
    getCpaIntegration: async () => integration({
      configured: false,
      runtimeOwned: false,
      runtimeSupported: true,
      installedVersion: null,
    }),
    getCpaRuntime: async () => {
      throw new Error("runtime down");
    },
  });
  assert.match(text(mounted.root), /加载 CPA 运行时失败: runtime down/);
  assert.equal(button(mounted.root, "托管安装").props.disabled, true);
  assert.doesNotMatch(text(mounted.root), /当前环境不支持托管 CPA 运行时/);
  mounted.app.unmount();
});

test("runtime polling stays serial while a request is in flight", async () => {
  let runtimeReads = 0;
  const pending = deferred<ReturnType<typeof runtime>>();
  const mounted = await mount({
    getCpaIntegration: async () => integration({ currentOperation: "install" }),
    getCpaRuntime: async () => {
      runtimeReads += 1;
      if (runtimeReads === 1) return runtime({ phase: "downloading", currentOperation: "install" });
      return pending.promise;
    },
    getCpaAccounts: async () => ({ accounts: [] }),
    getCpaRuntimeKeys: async () => ({ keys: [], processGeneration: 1, revision: 1 }),
  });
  assert.equal(runtimeReads, 1);
  await fireTimers(mounted.window);
  assert.equal(runtimeReads, 2);
  await fireTimers(mounted.window);
  assert.equal(runtimeReads, 2);
  pending.resolve(runtime({ phase: "downloading", currentOperation: "install" }));
  await settle();
  mounted.app.unmount();
});

test("stale runtime polls are ignored after a refresh", async () => {
  let runtimeReads = 0;
  const stale = deferred<ReturnType<typeof runtime>>();
  const mounted = await mount({
    getCpaIntegration: async () => integration(),
    getCpaRuntime: async () => {
      runtimeReads += 1;
      if (runtimeReads === 1) return runtime({ phase: "downloading", latestVersion: "1.0.0" });
      if (runtimeReads === 2) return stale.promise;
      return runtime({ phase: "downloading", latestVersion: "fresh-keep" });
    },
    getCpaAccounts: async () => ({ accounts: [] }),
    getCpaRuntimeKeys: async () => ({ keys: [], processGeneration: 1, revision: 1 }),
  });
  await fireTimers(mounted.window);
  assert.equal(runtimeReads, 2);
  await (button(mounted.root, "重试").props.onClick as () => Promise<void>)();
  await settle();
  stale.resolve(runtime({ phase: "idle", latestVersion: "stale-idle" }));
  await settle();
  assert.doesNotMatch(text(mounted.root), /stale-idle/);
  assert.match(text(mounted.root), /fresh-keep/);
  mounted.app.unmount();
});

test("a runtime poll failure stays visible with a local retry", async () => {
  let runtimeReads = 0;
  const mounted = await mount({
    getCpaIntegration: async () => integration(),
    getCpaRuntime: async () => {
      runtimeReads += 1;
      if (runtimeReads === 1) return runtime({ phase: "downloading" });
      if (runtimeReads === 2) throw new Error("poll failed");
      return runtime({ phase: "idle" });
    },
    getCpaAccounts: async () => ({ accounts: [] }),
    getCpaRuntimeKeys: async () => ({ keys: [], processGeneration: 1, revision: 1 }),
  });
  await fireTimers(mounted.window);
  await settle();
  assert.match(text(mounted.root), /CPA 运行时状态刷新失败: poll failed/);
  assert.match(text(mounted.root), /下载中/);
  const retries = mounted.root.children.flatMap(function walk(node: HostNode): HostNode[] {
    return [node, ...node.children.flatMap(walk)];
  }).filter((node) => node.type === "button" && text(node).trim() === "重试");
  assert.ok(retries.length >= 2);
  await (retries[retries.length - 1].props.onClick as () => Promise<void>)();
  await settle();
  assert.doesNotMatch(text(mounted.root), /CPA 运行时状态刷新失败/);
  assert.doesNotMatch(text(mounted.root), /下载中/);
  mounted.app.unmount();
});
