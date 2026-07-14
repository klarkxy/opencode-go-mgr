import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  APPLICATION_GUIDES,
  buildChatboxConfig,
  buildChatboxUrl,
  buildCherryStudioUrl,
} from "./application-guides.ts";
import {
  maskConnectionKey,
  normalizeClientRootUrl,
  resolveConnectionUrls,
  writeConnectionValue,
} from "./dashboard-connection.ts";

test("connection helpers mask display values and copy the complete value", async () => {
  assert.equal(maskConnectionKey(""), "未设置");
  assert.equal(maskConnectionKey("tinykey"), "ti…ey");
  assert.equal(maskConnectionKey("ocg-1234567890"), "ocg-…7890");

  let copied = "";
  await writeConnectionValue(async (value) => { copied = value; }, "ocg-secret-value");
  assert.equal(copied, "ocg-secret-value");
  await assert.rejects(() => writeConnectionValue(undefined, "value"), /剪贴板/);
});

test("client root normalization accepts roots and strips only a terminal v1", () => {
  assert.equal(normalizeClientRootUrl(""), "");
  assert.equal(normalizeClientRootUrl("   "), "");
  assert.equal(normalizeClientRootUrl(" https://ocg.example.com/// "), "https://ocg.example.com");
  assert.equal(normalizeClientRootUrl("https://ocg.example.com/proxy/"), "https://ocg.example.com/proxy");
  assert.equal(normalizeClientRootUrl("https://ocg.example.com/proxy/v1/"), "https://ocg.example.com/proxy");
  assert.equal(normalizeClientRootUrl("http://192.168.1.8:9042/ocg"), "http://192.168.1.8:9042/ocg");
});

test("client root normalization rejects endpoints and unsafe URL structure", () => {
  for (const value of [
    "ocg.example.com",
    "http:ocg.example.com",
    "http:/ocg.example.com",
    "/dashboard",
    "ftp://ocg.example.com",
    "https://user:password@ocg.example.com",
    "https://ocg.example.com?node=one",
    "https://ocg.example.com#settings",
    "https://ocg.example.com/v1/chat/completions",
    "https://ocg.example.com/proxy/v1/responses",
  ]) {
    assert.throws(() => normalizeClientRootUrl(value), Error, value);
  }
});

test("connection URL derivation handles configured, development, and production roots", () => {
  assert.deepEqual(
    resolveConnectionUrls("", "http://127.0.0.1:30001", 9042, true),
    {
      rootUrl: "http://127.0.0.1:9042",
      apiBaseUrl: "http://127.0.0.1:9042/v1",
      chatCompletionsUrl: "http://127.0.0.1:9042/v1/chat/completions",
      responsesUrl: "http://127.0.0.1:9042/v1/responses",
      messagesUrl: "http://127.0.0.1:9042/v1/messages",
      insecureHttp: false,
    },
  );
  assert.equal(
    resolveConnectionUrls("", "https://ocg.example.com", 9042, false).apiBaseUrl,
    "https://ocg.example.com/v1",
  );
  const configured = resolveConnectionUrls(
    "https://edge.example.com/ocg/v1/",
    "https://ignored.example.com",
    9042,
    false,
  );
  assert.equal(configured.rootUrl, "https://edge.example.com/ocg");
  assert.equal(configured.apiBaseUrl, "https://edge.example.com/ocg/v1");
  assert.doesNotMatch(configured.apiBaseUrl, /\/v1\/v1/);
  assert.equal(resolveConnectionUrls("http://localhost:9042", "https://ignored", 9042, false).insecureHttp, false);
  assert.equal(resolveConnectionUrls("http://127.0.0.8:9042", "https://ignored", 9042, false).insecureHttp, false);
  assert.equal(resolveConnectionUrls("http://192.168.1.8:9042", "https://ignored", 9042, false).insecureHttp, true);
  assert.equal(resolveConnectionUrls("https://192.168.1.8:9042", "https://ignored", 9042, false).insecureHttp, false);
});

test("application catalog has nine verified clients and never displays a complete key", () => {
  assert.equal(APPLICATION_GUIDES.length, 9);
  assert.equal(new Set(APPLICATION_GUIDES.map((guide) => guide.id)).size, 9);
  assert.ok(APPLICATION_GUIDES.every((guide) => String(guide.id) !== "trae"));

  const actualKey = "ocg-this-is-the-complete-secret-key";
  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const context = {
    ...urls,
    displayKey: maskConnectionKey(actualKey),
    actualKey,
    modelId: "verified-model",
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  const expectedAddress = new Map([
    ["claude-code", urls.rootUrl],
    ["codex", urls.apiBaseUrl],
    ["opencode", urls.apiBaseUrl],
    ["cherry-studio", urls.rootUrl],
    ["vscode-copilot", urls.chatCompletionsUrl],
    ["cline", urls.apiBaseUrl],
    ["roo-code", urls.apiBaseUrl],
    ["continue", urls.apiBaseUrl],
    ["chatbox", urls.rootUrl],
  ]);

  for (const guide of APPLICATION_GUIDES) {
    const snippets = guide.snippets(context);
    assert.ok(snippets.length > 0, guide.id);
    assert.ok(snippets.every((snippet) => !snippet.display.includes(actualKey)), `${guide.id} display`);
    assert.ok(snippets.some((snippet) => snippet.copy.includes(actualKey)), `${guide.id} copy`);
    assert.ok(snippets.some((snippet) => snippet.copy.includes(context.modelId)), `${guide.id} model`);
    assert.ok(
      snippets.some((snippet) => snippet.copy.includes(expectedAddress.get(guide.id)!)),
      `${guide.id} address`,
    );
  }

  const claudeGuide = APPLICATION_GUIDES.find((guide) => guide.id === "claude-code");
  assert.ok(claudeGuide);
  const claudeSettings = JSON.parse(claudeGuide.snippets(context)[0].copy);
  for (const variable of [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
  ]) {
    assert.equal(claudeSettings.env[variable], context.modelId, variable);
  }
  assert.equal(claudeSettings.env.ANTHROPIC_CUSTOM_MODEL_OPTION, context.modelId);

  for (const appId of ["codex", "opencode"]) {
    const guide = APPLICATION_GUIDES.find((candidate) => candidate.id === appId);
    assert.ok(guide);
    const snippets = guide.snippets(context);
    assert.ok(snippets.some((snippet) => snippet.language === "powershell" && snippet.copy.includes(actualKey)));
    assert.ok(snippets.some((snippet) => snippet.language === "bash" && snippet.copy.includes(actualKey)));
  }
  const openCode = APPLICATION_GUIDES.find((guide) => guide.id === "opencode");
  assert.ok(openCode);
  const openCodeConfig = JSON.parse(openCode.snippets(context)[0].copy);
  assert.equal(openCodeConfig.provider.ocg.options.apiKey, "{env:OCG_API_KEY}");
  assert.doesNotMatch(openCode.snippets(context)[0].copy, new RegExp(actualKey));
});

test("Cherry Studio and Chatbox imports encode the exact key and selected model", () => {
  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const context = {
    ...urls,
    displayKey: "ocg-…7890",
    actualKey: "ocg-secret-key",
    modelId: "selected-model",
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  const decode = (value: string, parameter: string) => {
    const encoded = new URL(value).searchParams.get(parameter);
    assert.ok(encoded);
    return JSON.parse(Buffer.from(encoded, "base64").toString("utf8"));
  };

  const cherryUrl = buildCherryStudioUrl(context);
  assert.equal(new URL(cherryUrl).protocol, "cherrystudio:");
  assert.deepEqual(decode(cherryUrl, "data"), {
    id: "ocg-manager",
    name: "OCG Manager",
    type: "openai",
    baseUrl: context.rootUrl,
    apiKey: context.actualKey,
  });
  assert.doesNotMatch(cherryUrl, /sk-ocg-/);

  const chatboxConfig = buildChatboxConfig(context);
  assert.equal(chatboxConfig.id, `ocg-manager-${encodeURIComponent(context.rootUrl)}`);
  assert.equal(chatboxConfig.settings.apiHost, context.rootUrl);
  assert.equal(chatboxConfig.settings.apiPath, "/v1/chat/completions");
  assert.equal(chatboxConfig.settings.apiKey, context.actualKey);
  assert.equal(chatboxConfig.settings.models[0].modelId, context.modelId);
  assert.deepEqual(chatboxConfig.settings.models[0].capabilities, ["tool_use"]);
  const chatboxUrl = buildChatboxUrl(context);
  assert.equal(new URL(chatboxUrl).protocol, "chatbox:");
  assert.deepEqual(decode(chatboxUrl, "config"), chatboxConfig);
});

test("generated VS Code and Continue configs use their current complete shapes", () => {
  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const context = {
    ...urls,
    displayKey: "ocg-…7890",
    actualKey: "ocg-secret-key",
    modelId: "selected-model",
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  const vscode = APPLICATION_GUIDES.find((guide) => guide.id === "vscode-copilot")!;
  const vscodeConfig = JSON.parse(vscode.snippets(context)[0].copy);
  assert.equal(vscodeConfig[0].vendor, "customendpoint");
  assert.equal(vscodeConfig[0].apiType, "chat-completions");
  assert.equal(vscodeConfig[0].models[0].url, urls.chatCompletionsUrl);
  assert.equal(vscodeConfig[0].models[0].id, context.modelId);
  assert.equal(vscodeConfig[0].models[0].toolCalling, true);
  assert.equal(vscodeConfig[0].models[0].vision, false);
  assert.equal(vscodeConfig[0].models[0].maxInputTokens, 32768);
  assert.equal(vscodeConfig[0].models[0].maxOutputTokens, 8192);

  const continueGuide = APPLICATION_GUIDES.find((guide) => guide.id === "continue")!;
  const yaml = continueGuide.snippets(context)[0].copy;
  assert.match(yaml, /^name: OCG Manager\nversion: 1\.0\.0\nschema: v1\nmodels:/);
  assert.match(yaml, /model: "selected-model"/);
  assert.match(yaml, /useResponsesApi: false/);
  assert.match(yaml, /capabilities:\n\s+- tool_use/);
});

test("dashboard keeps the connection center first and protects key regeneration", async () => {
  const source = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.ok(template.indexOf("接入中心") < template.indexOf("kpi-row"));
  assert.match(template, /旧 Key 将立即失效/);
  assert.match(template, /:aria-label="t\('复制 API Base URL'\)"/);
  assert.match(template, /:aria-label="t\('刷新 Key'\)"/);
  assert.match(template, /\{\{ maskedKey \}\}/);
  assert.doesNotMatch(template, /<code>\{\{ serviceConfig\.gateway_key \}\}<\/code>/);
});

test("dashboard and settings keep partial data safe", async () => {
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");

  assert.match(dashboard, /Promise\.allSettled/);
  assert.match(settings, /:disabled="!loaded \|\| regenerating \|\| clientRootPreview\.status === 'error'"/);
  assert.match(settings, /if \(!loaded\.value\) return/);
  assert.match(settings, /\{\{ maskedSettingsKey \}\}/);
  assert.doesNotMatch(settings, /v-model:value="config\.gateway_key"/);
  assert.match(app, /mode === "register"[\s\S]*getAuthStatus\(\)[\s\S]*status\?\.initialized/);
});

test("applications view uses deep-linked subpages and a responsive second navigation", async () => {
  const applications = await readFile(new URL("./Applications.vue", import.meta.url), "utf8");
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");

  assert.match(applications, /DEFAULT_APPLICATION: ApplicationId = "claude-code"/);
  assert.match(applications, /url\.searchParams\.set\("app", value\)/);
  assert.match(applications, /window\.history\.pushState/);
  assert.match(applications, /<aside class="application-sider">/);
  assert.doesNotMatch(applications, /<n-layout/);
  assert.match(applications, /<n-menu/);
  assert.match(applications, /<n-select/);
  assert.match(applications, /tauriApi\.getApplicationModels\(\)/);
  assert.doesNotMatch(applications, /Authorization: `Bearer/);
  assert.doesNotMatch(applications, /\s+tag(?:\s|>)/);
  assert.doesNotMatch(applications, /fetch\(`\$\{connectionUrls\.value\.apiBaseUrl\}/);
  assert.doesNotMatch(applications, /<n-tabs/);
  assert.doesNotMatch(applications, /<n-tab-pane/);
  assert.doesNotMatch(applications, /class="page-head"/);
  assert.doesNotMatch(applications, /class="guide-card"/);
  assert.match(applications, /\{\{ maskedKey \}\}/);
  assert.doesNotMatch(applications, /<code>\{\{ serviceConfig\.gateway_key \}\}<\/code>/);
  assert.match(app, /<main class="app-content">/);
  assert.doesNotMatch(app, /<n-layout-content/);
  assert.match(app, /dashboard.*accounts.*apps.*logs.*settings/s);
});

test("settings expose the downstream display root and bounded request timeouts", async () => {
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const api = await readFile(new URL("../api/tauri.ts", import.meta.url), "utf8");
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");

  assert.match(settings, /下游访问根地址（可选）/);
  assert.match(settings, /v-model:value="clientRootInputValue"/);
  assert.match(settings, /:readonly="config\.client_root_url_from_env"/);
  assert.match(settings, /:clearable="!config\.client_root_url_from_env && !!config\.client_root_url"/);
  assert.match(settings, /由环境变量 OCG_CLIENT_ROOT_URL 管理/);
  assert.match(settings, /v-else-if="!config\.client_root_url\.trim\(\)" class="sr-only"/);
  assert.match(settings, /\{\{ automaticClientRootFeedback \}\}/);
  assert.match(settings, /config\.client_root_url/);
  assert.match(settings, /client_root_url_from_env: false/);
  assert.match(settings, /get: \(\) => config\.value\.client_root_url,/);
  assert.match(settings, /:placeholder="config\.client_root_url_from_env \? '' : automaticClientRootUrls\.rootUrl"/);
  assert.doesNotMatch(settings, /config\.value\.client_root_url = resolveConnectionUrls/);
  assert.match(settings, /非本机 HTTP 会明文传输 Gateway Key 与请求内容/);
  assert.match(settings, /请求超时/);
  assert.match(settings, /config\.connect_timeout_secs"\s+:min="1"\s+:max="300"\s+:precision="0"/);
  assert.match(settings, /config\.non_stream_timeout_secs"\s+:min="1"\s+:max="3600"\s+:precision="0"/);
  assert.match(settings, /config\.stream_idle_timeout_secs"\s+:min="1"\s+:max="3600"\s+:precision="0"/);
  assert.match(settings, /connect_timeout_secs: 30/);
  assert.match(settings, /non_stream_timeout_secs: 120/);
  assert.match(settings, /stream_idle_timeout_secs: 300/);
  assert.match(settings, /if \(!timeoutsValid\(\)\)/);
  assert.match(api, /client_root_url: string/);
  assert.match(api, /client_root_url_from_env: boolean/);
  assert.match(api, /connect_timeout_secs: number/);
  assert.match(api, /non_stream_timeout_secs: number/);
  assert.match(api, /stream_idle_timeout_secs: number/);
  assert.doesNotMatch(dashboard, /ref<AppConfig>/);
});

test("settings expose supported Windows auto-start safely", async () => {
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const api = await readFile(new URL("../api/tauri.ts", import.meta.url), "utf8");

  assert.match(settings, /v-if="config\.auto_start_supported"/);
  assert.match(settings, /v-model:value="config\.auto_start"/);
  assert.match(settings, /:aria-label="t\('随 Windows 登录自动启动 OCG Manager'\)"/);
  assert.match(settings, /:disabled="!loaded \|\| saving \|\| regenerating"/);
  assert.match(settings, /:loading="regenerating"\s+:disabled="saving"/);
  assert.match(settings, /config\.value\.auto_start = persistedAutoStart/);
  assert.match(api, /auto_start_supported: boolean/);
});
