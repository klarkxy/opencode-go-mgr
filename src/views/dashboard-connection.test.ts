import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  APPLICATION_GUIDES,
  APPLICATION_MODEL_METADATA,
  buildChatboxConfig,
  buildChatboxUrl,
  buildCodexModelCatalog,
  recommendClaudeCodeModel,
  reconcileApplicationModelSelection,
} from "./application-guides.ts";
import {
  connectionDraftContextChanged,
  isGeminiCliBaseUrlAllowed,
  maskConnectionKey,
  normalizeClientRootUrl,
  reconcileConnectionDrafts,
  resolveConnectionUrls,
  restoreMaskedConnectionKey,
  writeConnectionValue,
} from "./dashboard-connection.ts";

test("connection draft context changes only when copied connection values change", () => {
  const previous = {
    gateway_port: 9042,
    gateway_key: "ocg-old-key",
    client_root_url: "https://old.example.com",
    upstream_base_url: "https://opencode.ai/zen/go",
  };

  assert.equal(connectionDraftContextChanged(previous, { ...previous }), false);
  for (const next of [
    { ...previous, gateway_port: 9043 },
    { ...previous, gateway_key: "ocg-new-key" },
    { ...previous, client_root_url: "https://new.example.com" },
    { ...previous, upstream_base_url: "https://upstream.example.com" },
  ]) {
    assert.equal(connectionDraftContextChanged(previous, next), true);
  }

  const drafts = { "codex:0": "edited" };
  assert.equal(reconcileConnectionDrafts(previous, { ...previous }, drafts), drafts);
  assert.deepEqual(
    reconcileConnectionDrafts(previous, { ...previous, gateway_key: "ocg-new-key" }, drafts),
    {},
  );
});

test("connection helpers mask display values and copy the complete value", async () => {
  assert.equal(maskConnectionKey(""), "未设置");
  assert.equal(maskConnectionKey("tinykey"), "ti…ey");
  assert.equal(maskConnectionKey("ocg-1234567890"), "ocg-…7890");

  let copied = "";
  await writeConnectionValue(async (value) => { copied = value; }, "ocg-secret-value");
  assert.equal(copied, "ocg-secret-value");
  await assert.rejects(() => writeConnectionValue(undefined, "value"), /剪贴板/);

  const specialKey = "ocg-$&-$$-$'-$`-tail";
  assert.equal(
    restoreMaskedConnectionKey('apiKey = "ocg-…tail"', "ocg-…tail", specialKey),
    `apiKey = "${specialKey}"`,
  );
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

test("Gemini CLI base URL compatibility allows HTTPS and exact loopback HTTP only", () => {
  for (const value of [
    "https://ocg.example.com",
    "https://192.168.1.8:9042/ocg",
    "http://localhost:9042",
    "http://127.0.0.1:9042",
    "http://[::1]:9042",
  ]) {
    assert.equal(isGeminiCliBaseUrlAllowed(value), true, value);
  }
  for (const value of [
    "http://192.168.1.8:9042",
    "http://127.0.0.8:9042",
    "http://gateway.localhost:9042",
    "ftp://localhost:9042",
    "not-a-url",
  ]) {
    assert.equal(isGeminiCliBaseUrlAllowed(value), false, value);
  }
});

test("model refresh preserves valid selections and falls back only when needed", () => {
  const available = ["model-a", "model-b", "model-c"];
  assert.deepEqual(
    reconcileApplicationModelSelection(
      ["model-a", "removed-model", "model-c", "model-a"],
      "model-c",
      available,
      available,
      true,
    ),
    { selectedModels: ["model-a", "model-c"], selectedModel: "model-c" },
  );
  assert.deepEqual(
    reconcileApplicationModelSelection(
      ["removed-model"],
      "removed-model",
      available,
      ["model-b", "model-c"],
      true,
    ),
    { selectedModels: ["model-b", "model-c"], selectedModel: "model-b" },
  );
  assert.deepEqual(
    reconcileApplicationModelSelection(undefined, undefined, available, ["model-b"], true),
    { selectedModels: ["model-b"], selectedModel: "model-b" },
  );
  assert.deepEqual(
    reconcileApplicationModelSelection([], "model-c", available, available, false),
    { selectedModels: [], selectedModel: "model-c" },
  );
  assert.deepEqual(
    reconcileApplicationModelSelection([], "removed-model", available, available, false),
    { selectedModels: [], selectedModel: "model-a" },
  );
});

test("application catalog has seventeen verified clients and never displays a complete key", () => {
  assert.equal(APPLICATION_GUIDES.length, 17);
  assert.equal(new Set(APPLICATION_GUIDES.map((guide) => guide.id)).size, 17);
  assert.ok(APPLICATION_GUIDES.every((guide) => String(guide.id) !== "trae"));
  const officialUrls = new Map([
    ["claude-code", "https://code.claude.com/docs/en/llm-gateway-connect"],
    ["claude-desktop", "https://claude.com/docs/third-party/claude-desktop/gateway"],
    ["codex", "https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers"],
    ["gemini-cli", "https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md"],
    ["pi", "https://pi.dev/docs/latest/models"],
    ["dsh", "https://github.com/deepseek-ai/deepseek-harness"],
    ["kimi-code", "https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/env-vars.html"],
    ["opencode", "https://opencode.ai/docs/providers/"],
    ["workbuddy", "https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model"],
    ["openclaw", "https://docs.openclaw.ai/start/wizard-cli-automation"],
    ["hermes", "https://hermes-agent.nousresearch.com/docs/integrations/providers"],
    ["cherry-studio", "https://docs.cherry-ai.com/en-us/pre-basic/providers/zi-ding-yi-fu-wu-shang"],
    ["vscode-copilot", "https://code.visualstudio.com/docs/agent-customization/language-models"],
    ["cline", "https://docs.cline.bot/provider-config/openai-compatible"],
    ["roo-code", "https://roocodeinc.github.io/Roo-Code/features/settings-management/"],
    ["continue", "https://docs.continue.dev/customize/model-providers/top-level/openai"],
    ["chatbox", "https://docs.chatboxai.app/en/guides/providers/import-config"],
  ]);
  for (const guide of APPLICATION_GUIDES) {
    assert.equal(guide.officialUrl, officialUrls.get(guide.id), `${guide.id} official docs`);
  }
  for (const appId of [
    "claude-code",
    "claude-desktop",
    "codex",
    "gemini-cli",
    "pi",
    "dsh",
    "kimi-code",
    "opencode",
    "workbuddy",
    "openclaw",
    "hermes",
  ]) {
    assert.ok(APPLICATION_GUIDES.some((guide) => guide.id === appId), appId);
  }

  for (const appId of ["claude-code", "claude-desktop"]) {
    assert.equal(
      APPLICATION_GUIDES.find((guide) => guide.id === appId)?.category,
      "Claude 兼容",
      appId,
    );
  }

  const actualKey = "ocg-this-is-the-complete-secret-key";
  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const modelValues = {
    ANTHROPIC_MODEL: "kimi-k3",
    ANTHROPIC_DEFAULT_FABLE_MODEL: "glm-5.1",
    ANTHROPIC_DEFAULT_HAIKU_MODEL: "kimi-k3",
    ANTHROPIC_DEFAULT_SONNET_MODEL: "glm-5.1",
    ANTHROPIC_DEFAULT_OPUS_MODEL: "kimi-k3",
    CLAUDE_CODE_SUBAGENT_MODEL: "glm-5.1",
    ANTHROPIC_CUSTOM_MODEL_OPTION: "kimi-k3",
    model: "kimi-k3",
    review_model: "glm-5.1",
  };
  const context = {
    ...urls,
    displayKey: maskConnectionKey(actualKey),
    actualKey,
    modelId: "kimi-k3",
    modelIds: ["kimi-k3", "glm-5.1"],
    availableModelIds: ["kimi-k3", "glm-5.1"],
    modelValues,
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  const expectedAddress = new Map([
    ["claude-code", urls.rootUrl],
    ["claude-desktop", `${urls.rootUrl}/claude-desktop`],
    ["codex", urls.apiBaseUrl],
    ["gemini-cli", urls.rootUrl],
    ["pi", urls.apiBaseUrl],
    ["dsh", urls.apiBaseUrl],
    ["kimi-code", urls.apiBaseUrl],
    ["opencode", urls.apiBaseUrl],
    ["workbuddy", urls.chatCompletionsUrl],
    ["openclaw", urls.apiBaseUrl],
    ["hermes", urls.apiBaseUrl],
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
    if (guide.id !== "claude-desktop") {
      assert.ok(snippets.some((snippet) => snippet.copy.includes(context.modelId)), `${guide.id} model`);
    }
    assert.ok(
      snippets.some((snippet) => snippet.copy.includes(expectedAddress.get(guide.id)!)),
      `${guide.id} address`,
    );
  }

  const claudeGuide = APPLICATION_GUIDES.find((guide) => guide.id === "claude-code");
  assert.ok(claudeGuide);
  const claudeSettings = JSON.parse(claudeGuide.snippets(context)[0].copy);
  assert.ok(!("CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY" in claudeSettings.env));
  for (const variable of [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
  ]) {
    assert.equal(claudeSettings.env[variable], modelValues[variable as keyof typeof modelValues], variable);
  }
  assert.equal(claudeSettings.env.ANTHROPIC_CUSTOM_MODEL_OPTION, modelValues.ANTHROPIC_CUSTOM_MODEL_OPTION);

  const codex = APPLICATION_GUIDES.find((guide) => guide.id === "codex");
  assert.ok(codex);
  assert.deepEqual(codex.modelFields, ["model", "review_model"]);
  const codexSnippets = codex.snippets(context);
  assert.equal(codexSnippets[0].label, "~/.codex/ocg-model-catalog.json");
  const codexCatalog = JSON.parse(codexSnippets[0].copy);
  const catalogModels = codexCatalog.models as Array<{
    slug: string;
    context_window: number;
    supported_reasoning_levels: unknown[];
  }>;
  assert.equal(catalogModels.length, 2);
  assert.deepEqual(
    catalogModels.map((model) => model.slug),
    ["kimi-k3", "glm-5.1"],
  );
  assert.equal(catalogModels[0]?.context_window, APPLICATION_MODEL_METADATA["kimi-k3"].contextWindow);
  assert.deepEqual(catalogModels.find((model) => model.slug === "glm-5.1")?.supported_reasoning_levels, []);
  assert.deepEqual(buildCodexModelCatalog(context), codexCatalog);
  assert.ok(codex.steps.some((step) => step.startsWith("可选：")));
  assert.equal(codexSnippets[1].label, "~/.codex/ocg.config.toml");
  assert.equal(codexSnippets[2].label, "~/.codex/config.toml");
  for (const label of ["~/.codex/ocg.config.toml", "~/.codex/config.toml"]) {
    const codexConfig = codexSnippets.find((snippet) => snippet.label === label)?.copy ?? "";
    assert.match(codexConfig, /model = "kimi-k3"/, label);
    assert.match(codexConfig, /review_model = "glm-5.1"/, label);
    assert.match(codexConfig, /model_provider = "ocg"/, label);
    assert.match(codexConfig, /^# model_catalog_json = "ocg-model-catalog.json"$/m, label);
    assert.doesNotMatch(codexConfig, /^model_catalog_json = /m, label);
    assert.match(codexConfig, /wire_api = "responses"/, label);
    assert.match(codexConfig, /requires_openai_auth = false/, label);
    assert.match(codexConfig, new RegExp(`base_url = "${urls.apiBaseUrl}"`), label);
  }
  assert.match(codexSnippets[2].copy, /Merge into user-level/);
  assert.ok(codexSnippets.some((snippet) => snippet.copy.includes("codex --profile ocg")));
  assert.ok(codexSnippets.some((snippet) => snippet.language === "powershell" && snippet.copy.includes(actualKey)));
  assert.ok(codexSnippets.some((snippet) => snippet.language === "bash" && snippet.copy.includes(actualKey)));

  const claudeDesktop = APPLICATION_GUIDES.find((guide) => guide.id === "claude-desktop");
  assert.ok(claudeDesktop);
  assert.deepEqual(claudeDesktop.modelFields, ["sonnet", "opus", "haiku"]);
  assert.ok(claudeDesktop.steps.some((step) => step.includes("Enable Developer Mode")));
  assert.ok(claudeDesktop.steps.some((step) => step.includes("桌面窗口不填模型 ID")));
  const desktopForm = claudeDesktop.snippets(context)[0].copy;
  assert.match(desktopForm, /Inference provider: Gateway/);
  assert.match(desktopForm, new RegExp(`Gateway base URL: ${urls.rootUrl}/claude-desktop`));
  assert.match(desktopForm, new RegExp(`Gateway API key: ${actualKey}`));

  const gemini = APPLICATION_GUIDES.find((guide) => guide.id === "gemini-cli");
  assert.ok(gemini);
  const geminiSnippets = gemini.snippets(context);
  const geminiEnv = geminiSnippets[0].copy;
  assert.match(geminiEnv, new RegExp(`GOOGLE_GEMINI_BASE_URL=${urls.rootUrl}`));
  assert.match(geminiEnv, /GOOGLE_GENAI_API_VERSION=v1beta/);
  assert.doesNotMatch(geminiEnv, /GEMINI_MODEL=/);
  const geminiSettings = JSON.parse(geminiSnippets[1].copy);
  assert.equal(geminiSettings.model.name, context.modelId);
  assert.deepEqual(geminiSettings.modelConfigs.customOverrides, [
    {
      match: { overrideScope: "core" },
      modelConfig: { model: context.modelId },
    },
  ]);
  assert.deepEqual(Object.keys(geminiSettings.agents.overrides), [
    "codebase_investigator",
    "cli_help",
    "generalist",
    "browser_agent",
  ]);
  for (const agent of Object.values(geminiSettings.agents.overrides) as Array<{
    modelConfig: { model: string };
  }>) {
    assert.equal(agent.modelConfig.model, context.modelId);
  }
  assert.doesNotMatch(geminiSnippets[1].copy, /"model":\s*"gemini-/);

  for (const appId of ["codex", "opencode"]) {
    const guide = APPLICATION_GUIDES.find((candidate) => candidate.id === appId);
    assert.ok(guide);
    const snippets = guide.snippets(context);
    assert.ok(snippets.some((snippet) => snippet.language === "powershell" && snippet.copy.includes(actualKey)));
    assert.ok(snippets.some((snippet) => snippet.language === "bash" && snippet.copy.includes(actualKey)));
  }
  for (const appId of ["pi", "dsh"]) {
    const guide = APPLICATION_GUIDES.find((candidate) => candidate.id === appId);
    assert.ok(guide);
    const snippets = guide.snippets(context);
    assert.ok(snippets.some((snippet) => snippet.copy.includes(actualKey)), `${appId} native credential`);
    assert.ok("badge" in guide && guide.badge === "原生插件", `${appId} native plugin badge`);
  }
  const openCode = APPLICATION_GUIDES.find((guide) => guide.id === "opencode");
  assert.ok(openCode);
  const openCodeConfig = JSON.parse(openCode.snippets(context)[0].copy);
  assert.equal(openCodeConfig.provider.ocg.options.apiKey, "{env:OCG_API_KEY}");
  assert.deepEqual(Object.keys(openCodeConfig.provider.ocg.models), context.modelIds);
  for (const modelId of context.modelIds) {
    assert.deepEqual(openCodeConfig.provider.ocg.models[modelId], {
      name: modelId,
      reasoning: true,
    });
  }
  assert.doesNotMatch(openCode.snippets(context)[0].copy, new RegExp(actualKey));
  assert.equal(openCode.snippets(context)[0].label, "~/.config/opencode/ocg.json");
  assert.ok(openCode.snippets(context).some((snippet) => snippet.copy.includes("OPENCODE_CONFIG")));
  assert.ok(openCode.snippets(context).some((snippet) => snippet.copy.includes("\nopencode")));

  const workBuddy = APPLICATION_GUIDES.find((guide) => guide.id === "workbuddy");
  assert.ok(workBuddy);
  assert.ok(!("multipleModels" in workBuddy));
  const workBuddyForm = workBuddy.snippets(context)[0].copy;
  assert.match(workBuddyForm, /Provider: Custom/);
  assert.match(workBuddyForm, new RegExp(`URL: ${urls.chatCompletionsUrl}`));
  assert.match(workBuddyForm, new RegExp(`API Key: ${actualKey}`));
  assert.match(workBuddyForm, /Model: kimi-k3/);
  assert.match(workBuddyForm, /Custom Protocol: Off/);
  assert.match(workBuddyForm, /Tool Calling: On/);
  assert.match(workBuddyForm, /Image Input: On/);
  assert.match(workBuddyForm, /Reasoning Mode: On/);

  const pi = APPLICATION_GUIDES.find((guide) => guide.id === "pi");
  assert.ok(pi);
  const piConfig = JSON.parse(pi.snippets(context)[0].copy);
  assert.equal(piConfig.providers.ocg.baseUrl, urls.apiBaseUrl);
  assert.equal(piConfig.providers.ocg.api, "openai-completions");
  assert.ok(!("apiKey" in piConfig.providers.ocg));
  assert.deepEqual(piConfig.providers.ocg.compat, {
    supportsStore: false,
    supportsDeveloperRole: false,
    maxTokensField: "max_tokens",
  });
  const piModels = piConfig.providers.ocg.models as Array<{ id: string; contextWindow: number }>;
  assert.equal(piModels.length, 2);
  assert.deepEqual(piModels.map((model) => model.id), ["kimi-k3", "glm-5.1"]);
  assert.equal(piModels[0]?.contextWindow, 1_048_576);

  const kimiCode = APPLICATION_GUIDES.find((guide) => guide.id === "kimi-code");
  assert.ok(kimiCode);
  const kimiSnippets = kimiCode.snippets(context);
  const kimiPowerShell = kimiSnippets.find((snippet) => snippet.language === "powershell")!.copy;
  const kimiBash = kimiSnippets.find((snippet) => snippet.language === "bash")!.copy;
  const kimiConfig = kimiSnippets.find((snippet) => snippet.language === "toml")!.copy;
  for (const command of [kimiPowerShell, kimiBash]) {
    assert.match(command, /KIMI_MODEL_NAME/);
    assert.match(command, /KIMI_MODEL_PROVIDER_TYPE/);
    assert.match(command, /KIMI_MODEL_BASE_URL/);
    assert.match(command, /KIMI_MODEL_MAX_CONTEXT_SIZE/);
    assert.match(command, /KIMI_MODEL_CAPABILITIES/);
  }
  assert.match(kimiConfig, /\[providers\.ocg\]\ntype = "openai"/);
  assert.match(kimiConfig, new RegExp(`base_url = ${JSON.stringify(urls.apiBaseUrl)}`));
  assert.match(kimiConfig, new RegExp(`api_key = ${JSON.stringify(actualKey)}`));
  assert.match(kimiConfig, /default_permission_mode = "manual"/);
  assert.ok(context.modelIds.every((modelId) => kimiConfig.includes(`[models."ocg\/${modelId}"]`)));
  assert.match(kimiConfig, /\[models\."ocg\/kimi-k3"\][\s\S]*?max_context_size = 1048576/);
  assert.match(
    kimiConfig,
    /capabilities = \["thinking","always_thinking","image_in","video_in","tool_use"\]/,
  );
  assert.match(kimiConfig, /support_efforts = \["max"\]\ndefault_effort = "max"/);

  const openClaw = APPLICATION_GUIDES.find((guide) => guide.id === "openclaw");
  assert.ok(openClaw);
  const openClawSnippets = openClaw.snippets(context);
  const openClawConfigSnippet = openClawSnippets.find((snippet) => snippet.language === "json5")!;
  const openClawEnv = openClawSnippets.find((snippet) => snippet.label === "~/.openclaw/.env")!;
  const openClawOnboarding = openClawSnippets.find((snippet) => snippet.language === "powershell")!.copy;
  const openClawConfig = JSON.parse(openClawConfigSnippet.copy);
  assert.equal(openClawConfig.models.providers.ocg.apiKey, "${CUSTOM_API_KEY}");
  assert.deepEqual(openClawConfig.models.providers.ocg.models[0], {
    id: "kimi-k3",
    name: "kimi-k3",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1_048_576,
    maxTokens: 131_072,
  });
  assert.doesNotMatch(openClawConfigSnippet.copy, new RegExp(actualKey));
  assert.equal(openClawEnv.copy, `CUSTOM_API_KEY=${JSON.stringify(actualKey)}`);
  assert.match(openClawOnboarding, /openclaw onboard --non-interactive --accept-risk/);
  assert.match(openClawOnboarding, /--secret-input-mode ref/);
  assert.match(openClawOnboarding, /--custom-compatibility openai/);
  assert.match(openClawOnboarding, /--custom-image-input/);
  assert.doesNotMatch(openClawOnboarding, /--custom-api-key/);

  const hermes = APPLICATION_GUIDES.find((guide) => guide.id === "hermes")!;
  const hermesConfig = hermes.snippets(context)[0].copy;
  assert.match(hermesConfig, /^providers:\n  ocg:\n    api: /);
  assert.match(hermesConfig, /key_env: OCG_API_KEY/);
  assert.match(hermesConfig, /transport: chat_completions/);
  assert.match(hermesConfig, /provider: custom:ocg/);
  assert.doesNotMatch(hermesConfig, /custom_providers:/);
  assert.doesNotMatch(hermesConfig, /api_mode:/);
  assert.match(hermesConfig, /"kimi-k3":\n\s+context_length: 1048576\n\s+supports_vision: true/);
  assert.match(hermesConfig, /"glm-5\.1":\n\s+context_length: 202752\n\s+supports_vision: false/);

  assert.deepEqual(
    APPLICATION_GUIDES.filter((guide) => "popular" in guide && guide.popular).map((guide) => guide.id),
    ["openclaw", "hermes"],
  );

  const rooCode = APPLICATION_GUIDES.find((guide) => guide.id === "roo-code")!;
  const rooAutoImport = JSON.parse(
    rooCode.snippets(context).find((snippet) => snippet.label.startsWith("VS Code settings.json"))!.copy,
  );
  assert.equal(rooAutoImport["roo-cline.autoImportSettingsPath"], "~/roo-code-settings.json");

  for (const appId of [
    "opencode",
    "pi",
    "kimi-code",
    "openclaw",
    "hermes",
    "cherry-studio",
    "vscode-copilot",
    "continue",
    "chatbox",
  ]) {
    const guide = APPLICATION_GUIDES.find((candidate) => candidate.id === appId);
    assert.ok(guide && "multipleModels" in guide && guide.multipleModels, appId);
    const config = guide.snippets(context).map(({ copy }) => copy).join("\n");
    assert.ok(context.modelIds.every((modelId) => config.includes(modelId)), appId);
  }
});

test("Pi and Kimi Code configs use verified per-model limits and capabilities without fallback guesses", () => {
  const expected = new Map<string, {
    contextWindow: number;
    maxOutputTokens: number;
    piInput: readonly string[];
    kimiCapabilities: readonly string[];
  }>([
    ["grok-4.5", { contextWindow: 500_000, maxOutputTokens: 500_000, piInput: ["text", "image"], kimiCapabilities: ["thinking", "always_thinking", "image_in", "tool_use"] }],
    ["gpt-5.6-luna", { contextWindow: 1_050_000, maxOutputTokens: 128_000, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "tool_use"] }],
    ["muse-spark-1.2", { contextWindow: 1_048_576, maxOutputTokens: 131_072, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "tool_use"] }],
    ["muse-spark-1.2-contributor", { contextWindow: 1_048_576, maxOutputTokens: 131_072, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "tool_use"] }],
    ["glm-5.3", { contextWindow: 1_000_000, maxOutputTokens: 131_072, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["glm-5.2", { contextWindow: 1_000_000, maxOutputTokens: 131_072, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["glm-5.1", { contextWindow: 202_752, maxOutputTokens: 32_768, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["kimi-k3", { contextWindow: 1_048_576, maxOutputTokens: 131_072, piInput: ["text", "image"], kimiCapabilities: ["thinking", "always_thinking", "image_in", "video_in", "tool_use"] }],
    ["kimi-k2.7-code", { contextWindow: 262_144, maxOutputTokens: 262_144, piInput: ["text", "image"], kimiCapabilities: ["thinking", "always_thinking", "image_in", "video_in", "tool_use"] }],
    ["kimi-k2.6", { contextWindow: 262_144, maxOutputTokens: 65_536, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "video_in", "tool_use"] }],
    ["mimo-v2.5", { contextWindow: 1_000_000, maxOutputTokens: 128_000, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "video_in", "audio_in", "tool_use"] }],
    ["mimo-v2.5-pro", { contextWindow: 1_048_576, maxOutputTokens: 128_000, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["minimax-m3", { contextWindow: 1_000_000, maxOutputTokens: 131_072, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "tool_use"] }],
    ["minimax-m2.7", { contextWindow: 204_800, maxOutputTokens: 131_072, piInput: ["text"], kimiCapabilities: ["thinking", "always_thinking", "tool_use"] }],
    ["minimax-m2.7-highspeed", { contextWindow: 204_800, maxOutputTokens: 131_072, piInput: ["text"], kimiCapabilities: ["thinking", "always_thinking", "tool_use"] }],
    ["minimax-m2.5", { contextWindow: 204_800, maxOutputTokens: 65_536, piInput: ["text"], kimiCapabilities: ["thinking", "always_thinking", "tool_use"] }],
    ["minimax-m2.5-highspeed", { contextWindow: 204_800, maxOutputTokens: 65_536, piInput: ["text"], kimiCapabilities: ["thinking", "always_thinking", "tool_use"] }],
    ["qwen3.8-max", { contextWindow: 1_000_000, maxOutputTokens: 131_072, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["qwen3.7-max", { contextWindow: 1_000_000, maxOutputTokens: 65_536, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["qwen3.7-plus", { contextWindow: 1_000_000, maxOutputTokens: 65_536, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "tool_use"] }],
    ["qwen3.6-plus", { contextWindow: 1_000_000, maxOutputTokens: 65_536, piInput: ["text", "image"], kimiCapabilities: ["thinking", "image_in", "tool_use"] }],
    ["deepseek-v4-pro", { contextWindow: 1_000_000, maxOutputTokens: 384_000, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["deepseek-v4-flash", { contextWindow: 1_000_000, maxOutputTokens: 384_000, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
    ["hy3", { contextWindow: 256_000, maxOutputTokens: 64_000, piInput: ["text"], kimiCapabilities: ["thinking", "tool_use"] }],
  ]);
  assert.deepEqual(new Set(Object.keys(APPLICATION_MODEL_METADATA)), new Set(expected.keys()));
  for (const [modelId, spec] of expected) {
    assert.equal(APPLICATION_MODEL_METADATA[modelId].contextWindow, spec.contextWindow, `${modelId} context`);
    assert.equal(APPLICATION_MODEL_METADATA[modelId].maxOutputTokens, spec.maxOutputTokens, `${modelId} output`);
  }

  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const context = {
    ...urls,
    displayKey: "ocg-…7890",
    actualKey: "ocg-secret-key",
    modelId: expected.keys().next().value!,
    modelIds: [...expected.keys()],
    availableModelIds: [...expected.keys()],
    modelValues: {},
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };

  const pi = APPLICATION_GUIDES.find((guide) => guide.id === "pi")!;
  const piConfig = JSON.parse(pi.snippets(context)[0].copy);
  assert.deepEqual(piConfig.providers.ocg.compat, {
    supportsStore: false,
    supportsDeveloperRole: false,
    maxTokensField: "max_tokens",
  });
  assert.equal(piConfig.providers.ocg.models.length, expected.size);
  for (const model of piConfig.providers.ocg.models) {
    const spec = expected.get(model.id)!;
    assert.equal(model.contextWindow, spec.contextWindow, `${model.id} Pi context`);
    assert.equal(model.maxTokens, spec.maxOutputTokens, `${model.id} Pi output`);
    assert.deepEqual(model.input, spec.piInput, `${model.id} Pi input`);
    assert.equal(model.reasoning, true, `${model.id} Pi reasoning`);
  }
  const piModels = new Map<string, Record<string, unknown>>(
    piConfig.providers.ocg.models.map((model: Record<string, unknown>) => [String(model.id), model] as const),
  );
  assert.deepEqual(piModels.get("kimi-k2.6")!.compat, {
    thinkingFormat: "deepseek",
    supportsReasoningEffort: false,
    supportsLongCacheRetention: false,
  });
  assert.deepEqual(piModels.get("kimi-k2.6")!.thinkingLevelMap, {
    minimal: null,
    low: null,
    medium: null,
  });
  assert.deepEqual(piModels.get("kimi-k2.7-code")!.compat, {
    supportsReasoningEffort: false,
  });
  assert.deepEqual(piModels.get("kimi-k2.7-code")!.thinkingLevelMap, {
    off: null,
    minimal: null,
    low: null,
    medium: null,
    xhigh: null,
    max: null,
  });
  assert.deepEqual(piModels.get("glm-5.3")!.thinkingLevelMap, {
    off: null,
    minimal: null,
    low: "low",
    medium: null,
    high: "high",
    xhigh: null,
    max: "max",
  });
  assert.deepEqual(piModels.get("glm-5.2")!.thinkingLevelMap, {
    off: null,
    minimal: null,
    low: null,
    medium: null,
    high: "high",
    xhigh: null,
    max: "max",
  });
  assert.deepEqual(piModels.get("glm-5.1")!.compat, { supportsReasoningEffort: false });
  assert.deepEqual(piModels.get("glm-5.1")!.thinkingLevelMap, {
    off: null,
    minimal: null,
    low: null,
    medium: null,
    xhigh: null,
    max: null,
  });
  for (const modelId of [
    "minimax-m2.7",
    "minimax-m2.7-highspeed",
    "minimax-m2.5",
    "minimax-m2.5-highspeed",
  ]) {
    assert.deepEqual(piModels.get(modelId)!.compat, { supportsReasoningEffort: false }, modelId);
    assert.deepEqual(piModels.get(modelId)!.thinkingLevelMap, {
      off: null,
      minimal: null,
      low: null,
      medium: null,
      xhigh: null,
      max: null,
    }, modelId);
  }
  for (const modelId of ["minimax-m3", "qwen3.8-max", "qwen3.7-max", "qwen3.7-plus", "qwen3.6-plus"]) {
    assert.equal(piModels.get(modelId)!.compat, undefined, modelId);
    assert.deepEqual(piModels.get(modelId)!.thinkingLevelMap, { minimal: "low" }, modelId);
  }

  const kimiCode = APPLICATION_GUIDES.find((guide) => guide.id === "kimi-code")!;
  const kimiConfig = kimiCode.snippets(context).find((snippet) => snippet.language === "toml")!.copy;
  assert.doesNotMatch(kimiConfig, /max_context_size = 128000(?:\r?\n|$)/);
  assert.doesNotMatch(kimiConfig, /max_output_size/);
  const kimiTables = new Map<string, string>();
  for (const [modelId, spec] of expected) {
    const header = `[models.${JSON.stringify(`ocg/${modelId}`)}]`;
    const start = kimiConfig.indexOf(header);
    assert.notEqual(start, -1, `${modelId} Kimi table`);
    const next = kimiConfig.indexOf("\n\n[models.", start + header.length);
    const table = kimiConfig.slice(start, next === -1 ? undefined : next);
    kimiTables.set(modelId, table);
    assert.match(table, new RegExp(`max_context_size = ${spec.contextWindow}(?:\\r?\\n|$)`), `${modelId} Kimi context`);
    assert.ok(
      table.includes(`capabilities = ${JSON.stringify(spec.kimiCapabilities)}`),
      `${modelId} Kimi capabilities`,
    );
  }
  assert.match(kimiTables.get("grok-4.5")!, /support_efforts = \["low","medium","high"\]\ndefault_effort = "high"/);
  assert.match(kimiTables.get("glm-5.3")!, /support_efforts = \["low","high","max"\]\ndefault_effort = "max"/);
  assert.match(kimiTables.get("glm-5.2")!, /support_efforts = \["high","max"\]\ndefault_effort = "max"/);
  assert.match(kimiTables.get("kimi-k3")!, /support_efforts = \["max"\]\ndefault_effort = "max"/);
  for (const modelId of ["deepseek-v4-pro", "deepseek-v4-flash"]) {
    assert.match(kimiTables.get(modelId)!, /support_efforts = \["high","max"\]\ndefault_effort = "high"/);
  }

  const unknownContext = { ...context, modelId: "future-unverified-model", modelIds: ["future-unverified-model"] };
  assert.throws(
    () => pi.snippets(unknownContext),
    /Missing verified application model metadata for "future-unverified-model"/,
  );
  assert.throws(
    () => kimiCode.snippets(unknownContext),
    /Missing verified application model metadata for "future-unverified-model"/,
  );
});

test("Claude Code defaults prefer Messages-capable models with safe fallbacks", () => {
  const models = [
    "glm-5.2",
    "kimi-k2.7-code",
    "kimi-k3",
    "deepseek-v4-flash",
    "minimax-m3",
    "qwen3.7-max",
    "qwen3.7-plus",
  ];
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_MODEL", models), "qwen3.7-plus");
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_DEFAULT_FABLE_MODEL", models), "qwen3.7-max");
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_DEFAULT_HAIKU_MODEL", models), "deepseek-v4-flash");
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_DEFAULT_SONNET_MODEL", models), "qwen3.7-plus");
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_DEFAULT_OPUS_MODEL", models), "glm-5.2");
  assert.equal(recommendClaudeCodeModel("CLAUDE_CODE_SUBAGENT_MODEL", models), "minimax-m3");
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_CUSTOM_MODEL_OPTION", models), "kimi-k3");
  const claudeGuide = APPLICATION_GUIDES.find((guide) => guide.id === "claude-code")!;
  for (const field of claudeGuide.modelFields ?? []) {
    assert.equal(
      recommendClaudeCodeModel(field, ["kimi-k2.7-code", "mimo-v2.5", "kimi-k3"]),
      "kimi-k3",
      field,
    );
  }
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_DEFAULT_HAIKU_MODEL", ["mimo-v2.5"]), "mimo-v2.5");
  assert.equal(recommendClaudeCodeModel("unknown", ["fallback-model"]), "fallback-model");
  assert.equal(recommendClaudeCodeModel("ANTHROPIC_MODEL", []), "");
});

test("dotenv snippets quote keys containing comments and replacement tokens", () => {
  const actualKey = "ocg-$&-$$-#fragment";
  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const context = {
    ...urls,
    displayKey: maskConnectionKey(actualKey),
    actualKey,
    modelId: "kimi-k3",
    modelIds: ["kimi-k3"],
    availableModelIds: ["kimi-k3"],
    modelValues: {},
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  const gemini = APPLICATION_GUIDES.find((guide) => guide.id === "gemini-cli")!;
  const hermes = APPLICATION_GUIDES.find((guide) => guide.id === "hermes")!;
  assert.ok(gemini.snippets(context)[0].copy.startsWith(`GEMINI_API_KEY=${JSON.stringify(actualKey)}\n`));
  assert.equal(hermes.snippets(context)[1].copy, `OCG_API_KEY=${JSON.stringify(actualKey)}`);
});

test("Chatbox import encodes the exact key and every selected model", () => {
  const urls = resolveConnectionUrls("https://edge.example.com/ocg", "https://ignored", 9042, false);
  const context = {
    ...urls,
    displayKey: "ocg-…7890",
    actualKey: "ocg-secret-key",
    modelId: "selected-model",
    modelIds: ["selected-model", "second-model"],
    availableModelIds: ["selected-model", "second-model"],
    modelValues: {},
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  const decode = (value: string, parameter: string) => {
    const encoded = new URL(value).searchParams.get(parameter);
    assert.ok(encoded);
    return JSON.parse(Buffer.from(encoded, "base64").toString("utf8"));
  };

  const chatboxConfig = buildChatboxConfig(context);
  assert.equal(chatboxConfig.id, `ocg-manager-${encodeURIComponent(context.rootUrl)}`);
  assert.equal(chatboxConfig.settings.apiHost, context.rootUrl);
  assert.equal(chatboxConfig.settings.apiPath, "/v1/chat/completions");
  assert.equal(chatboxConfig.settings.apiKey, context.actualKey);
  assert.equal(chatboxConfig.settings.models[0].modelId, context.modelId);
  assert.deepEqual(chatboxConfig.settings.models.map(({ modelId }) => modelId), context.modelIds);
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
    modelIds: ["selected-model", "second-model"],
    availableModelIds: ["selected-model", "second-model"],
    modelValues: {},
    iconUrl: "https://edge.example.com/dashboard/ocg.png",
  };
  // Expected token budgets come from APPLICATION_MODEL_METADATA (asserted in the
  // per-model limits test), so this test only verifies how the guide splits them.
  const vscodeModelIds = [
    "glm-5.2",
    "glm-5.1",
    "kimi-k2.7-code",
    "kimi-k2.6",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "mimo-v2.5",
    "mimo-v2.5-pro",
    "minimax-m3",
    "minimax-m2.7",
    "minimax-m2.5",
    "qwen3.7-max",
    "qwen3.7-plus",
    "qwen3.6-plus",
  ];
  const vscodeContext = {
    ...context,
    modelId: "glm-5.2",
    modelIds: vscodeModelIds,
    availableModelIds: vscodeModelIds,
  };
  const vscode = APPLICATION_GUIDES.find((guide) => guide.id === "vscode-copilot")!;
  const vscodeConfig = JSON.parse(vscode.snippets(vscodeContext)[0].copy);
  assert.equal(vscodeConfig[0].vendor, "customendpoint");
  assert.equal(vscodeConfig[0].apiType, "chat-completions");
  assert.equal(vscodeConfig[0].models[0].url, urls.chatCompletionsUrl);
  assert.equal(vscodeConfig[0].models[0].id, vscodeContext.modelId);
  assert.equal(vscodeConfig[0].models[0].toolCalling, true);
  assert.equal(vscodeConfig[0].models[0].vision, false);
  assert.deepEqual(vscodeConfig[0].models.map((model: { id: string }) => model.id), vscodeContext.modelIds);
  for (const model of vscodeConfig[0].models) {
    const metadata = APPLICATION_MODEL_METADATA[model.id];
    assert.equal(
      model.maxInputTokens + model.maxOutputTokens,
      metadata.contextWindow,
      model.id,
    );
    assert.equal(model.maxOutputTokens, metadata.maxOutputTokens, model.id);
    assert.equal(model.vision, (metadata.ocgInput ?? metadata.input).includes("image"), model.id);
  }

  const continueGuide = APPLICATION_GUIDES.find((guide) => guide.id === "continue")!;
  const yaml = continueGuide.snippets(context)[0].copy;
  assert.match(yaml, /^name: OCG Manager\nversion: 1\.0\.0\nschema: v1\nmodels:/);
  assert.match(yaml, /model: "selected-model"/);
  assert.match(yaml, /model: "second-model"/);
  assert.match(yaml, /apiKey: \$\{\{ secrets\.OCG_API_KEY \}\}/);
  assert.doesNotMatch(yaml, new RegExp(context.actualKey));
  assert.match(yaml, /useResponsesApi: false/);
  assert.match(yaml, /capabilities:\n\s+- tool_use/);
  assert.equal(
    continueGuide.snippets(context).find((snippet) => snippet.language === "dotenv")!.copy,
    `OCG_API_KEY=${JSON.stringify(context.actualKey)}`,
  );
});

test("dashboard keeps the connection center first and protects key regeneration", async () => {
  const source = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const template = source.slice(source.indexOf("<template>"), source.indexOf("<script setup"));

  assert.ok(template.indexOf("接入中心") < template.indexOf("attention-card"));
  assert.doesNotMatch(template, /kpi-row|kpi-card|用量摘要|可路由账号/);
  // Regeneration is per selected key: only that key's old value dies.
  assert.match(template, /仅当前 Key 的旧值立即失效/);
  assert.match(template, /:aria-label="t\('复制 API Base URL'\)"/);
  assert.match(template, /:aria-label="t\('刷新 Key'\)"/);
  assert.match(template, /copyConnection\('api', serviceApiUrl, t\('API 地址'\)\)/);
  assert.match(template, /copyConnection\('key', selectedKey\?\.value \?\? '', t\('Key'\)\)/);
  assert.match(template, /:aria-label="t\('管理接入 Key'\)"/);
  assert.match(template, /@click="goToKeys"/);
  assert.doesNotMatch(template, /<span class="sr-only">(?:API Base URL|Key)<\/span>/);
  assert.match(template, /\{\{ maskedKey \}\}/);
  assert.doesNotMatch(template, /<code>\{\{ serviceConfig\.gateway_key \}\}<\/code>/);
  // The switcher renders only with more than one credential: with a single
  // (primary) key the layout matches the pre-multi-key single-key row.
  assert.match(template, /v-if="enabledGatewayKeys\.length > 1"/);
  assert.match(template, /class="key-switcher-trigger"/);
  assert.match(template, /:aria-label="t\('选择 Key'\)"/);
  assert.doesNotMatch(template, /connection-key-select/);
  assert.match(source, /watch\(enabledGatewayKeys, \(keys\) =>/);
  assert.match(source, /selectedKeyId\.value = keys\[0\]\.id/);
  // The connection center consumes the lightweight ConnectionInfo payload,
  // never the full settings shape.
  assert.match(source, /import type \{\s*Account,\s*ConnectionInfo,/);
  assert.doesNotMatch(source, /ref<AppConfig>/);
  // The primary key is pinned first and identified by the fixed constant.
  assert.match(source, /const enabledGatewayKeys = computed<SwitcherKey\[\]>\(\(\) => \[\s*\{ id: PRIMARY_KEY_ID, name: t\("主 Key"\), value: serviceConfig\.value\.primary_key \},/);
});

test("dashboard reports gateway health and serializes key regeneration", async () => {
  const source = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");

  assert.match(source, /'not-ready': summaryLoaded && !summary\.gateway_running/);
  assert.match(source, /!summaryLoaded \? t\("加载中…"\) : summary\.gateway_running \? t\("就绪"\) : t\("服务未就绪"\)/);
  assert.match(source, /gateway_running: false/);
  assert.match(source, /const summaryLoaded = ref\(false\)/);
  assert.match(source, /v-if="dashboardError"[\s\S]*?@click="loadDashboard"/);
  assert.match(source, /:disabled="refreshingKey \|\| !selectedKey"/);
  assert.match(source, /:loading="refreshingKey"\s+:disabled="refreshingKey \|\| loading \|\| !selectedKey"/);
  assert.match(source, /async function regenerateKey\(\) \{\s*const target = selectedKey\.value;\s*if \(refreshingKey\.value \|\| dashboardRequestActive \|\| !target\) return;/);
  assert.match(source, /async function loadDashboard\(\) \{\s*if \(dashboardRequestActive \|\| refreshingKey\.value\) return;/);
  assert.match(source, /emit\("navigate", "keys"\)/);
  assert.match(source, /function goToKeys\(\) \{\s*emit\("navigate", "keys"\);/);
  // Both primary and sub-key rotations go through the sole in-memory secret
  // owner so the page never keeps a second plaintext copy.
  assert.match(source, /const isPrimary = target\.id === PRIMARY_KEY_ID;/);
  assert.match(source, /connectionStore\.regeneratePrimaryKey\(\)/);
  assert.match(source, /connectionStore\.regenerateKey\(target\.id\)/);
  assert.match(source, /const serviceConfig = computed\(\(\) => connectionStore\.info \?\? EMPTY_CONNECTION\)/);
  assert.doesNotMatch(source, /serviceConfig\.value\.(?:primary_key|sub_keys)\s*=/);
});

test("app recovers first-run auth and contains intentional logout failures", async () => {
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");
  const onAuthRequired = app.slice(
    app.indexOf("function onAuthRequired"),
    app.indexOf("async function loadAuthStatus"),
  );
  const submitAuth = app.slice(
    app.indexOf("async function submitAuth"),
    app.indexOf("async function logout"),
  );
  const logout = app.slice(
    app.indexOf("async function logout"),
    app.indexOf("watch(activeKey"),
  );

  assert.match(onAuthRequired, /if \(suppressAuthRequired\) return;/);
  assert.match(submitAuth, /mode === "login" && e instanceof DashboardRequestError && e\.status === 401[\s\S]*session\.loadStatus\(\)[\s\S]*!status\.initialized[\s\S]*authState\.value = "register"/);
  assert.match(logout, /^async function logout\(\) \{\s*if \(loggingOut\.value\) return;/);
  assert.match(logout, /suppressAuthRequired = true;[\s\S]*await session\.logout\(\)/);
  assert.match(logout, /catch \(e\) \{[\s\S]*logoutError\.value = t\("退出登录失败: \{error\}"/);
  assert.match(app, /v-if="logoutError"[\s\S]*\{\{ logoutError \}\}/);
});

test("dashboard and settings keep partial data safe", async () => {
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");

  assert.match(dashboard, /Promise\.allSettled/);
  assert.match(settings, /:disabled="!loaded \|\| testingProxy \|\| proxyUrlPreview\.status === 'error' \|\| clientRootPreview\.status === 'error' \|\| inviteUrlPreview\.status === 'error'"/);
  assert.match(settings, /if \(!loaded\.value\) return/);
  // Key lifecycle moved off Settings; the form must not render or edit keys.
  assert.doesNotMatch(settings, /class="settings-subsection gateway-keys"/);
  assert.doesNotMatch(settings, /v-model:value="config\.gateway_key"/);
  assert.doesNotMatch(settings, /maskConnectionKey/);
  assert.match(app, /mode === "register"[\s\S]*session\.loadStatus\(\)[\s\S]*status\?\.initialized/);
});

test("applications view uses deep-linked subpages and a responsive second navigation", async () => {
  const applications = await readFile(new URL("./Applications.vue", import.meta.url), "utf8");
  const app = await readFile(new URL("../App.vue", import.meta.url), "utf8");
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const restoreDefaults = applications.slice(
    applications.indexOf("function restoreApplicationDefaults"),
    applications.indexOf("async function copySnippet"),
  );
  const modelRowStart = applications.indexOf('<div class="model-row">');
  const modelRow = applications.slice(modelRowStart, applications.indexOf("</section>", modelRowStart));

  assert.match(applications, /DEFAULT_APPLICATION: ApplicationId = "claude-code"/);
  assert.match(applications, /url\.searchParams\.set\("app", value\)/);
  assert.match(applications, /window\.history\.pushState/);
  assert.match(applications, /<aside class="application-sider">/);
  assert.doesNotMatch(applications, /<n-layout/);
  assert.match(applications, /<n-menu/);
  assert.match(applications, /<n-select/);
  assert.match(applications, /v-if="activeGuide\.popular"[\s\S]*?t\("常用"\)/);
  assert.match(applications, /function guideOptionLabel[\s\S]*?guide\.popular[\s\S]*?t\("常用"\)/);
  assert.match(applications, /dashboardApi\.getApplicationModels\(\)/);
  assert.match(applications, /settingsStore\.loadClaudeDesktop\(\)/);
  // Connection fields come from the lightweight payload, not full settings.
  assert.match(applications, /connectionStore\.load\(\)/);
  assert.match(applications, /useConnectionStore\(\)/);
  assert.match(applications, /useSettingsStore\(\)/);
  assert.doesNotMatch(applications, /dashboardApi\.getSettings\(\)/);
  assert.doesNotMatch(applications, /t\("节点信息"\)|t\("服务地址"\)|t\("上游地址"\)/);
  assert.doesNotMatch(applications, /class="connection-track"|class="connection-stage"/);
  assert.match(applications, /class="access-fields"/);
  assert.match(applications, /enabledGatewayKeys/);
  assert.match(applications, /PRIMARY_KEY_ID/);
  assert.match(applications, /t\(['"]选择 Key['"]\)/);
  assert.match(applications, /<div class="access-field">[\s\S]*<n-select/);
  assert.doesNotMatch(applications, /<label class="access-field">[\s\S]*<n-select/);
  assert.match(applications, /selectedKey\.value\?\.value/);
  assert.match(applications, /Promise\.allSettled/);
  assert.match(applications, /const claudeDesktopModelsLoaded = ref\(false\)/);
  assert.match(applications, /activeGuide\.value\.id !== "claude-desktop" \|\| claudeDesktopModelsLoaded\.value/);
  assert.match(applications, /if \(guide\.id === "claude-desktop"\) continue/);
  assert.match(applications, /if \(!claudeDesktopModelsLoaded\.value\)[\s\S]*?return;/);
  assert.match(applications, /@click="loadModels"/);
  assert.match(modelRow, /@click="restoreApplicationDefaults"/);
  assert.match(modelRow, /@click="saveClaudeDesktopModels"/);
  assert.match(applications, /v-if="usesMuseContributor"/);
  assert.match(applications, /const selectedApplicationModelIds = computed/);
  assert.match(applications, /selectedApplicationModelIds\.value\.includes\("muse-spark-1\.2-contributor"\)/);
  assert.match(applications, /Muse Spark 1\.2 Contributor 不是 ZDR/);
  assert.equal(applications.match(/@click="restoreApplicationDefaults"/g)?.length, 1);
  // Dashboard KeepAlive visibility lifecycle: App caches Dashboard; bind on
  // mount/activation and unbind on deactivation/unmount.
  assert.match(app, /<KeepAlive>[\s\S]*?<Dashboard\b/);
  assert.match(dashboard, /onMounted\(\(\) => \{[\s\S]*?bindVisibilityRefresh\(\)/);
  assert.match(dashboard, /onActivated\(\(\) => \{[\s\S]*?bindVisibilityRefresh\(\)/);
  assert.match(dashboard, /onDeactivated\(\(\) => \{[\s\S]*?unbindVisibilityRefresh\(\)/);
  assert.match(dashboard, /onUnmounted\(\(\) => \{[\s\S]*?unbindVisibilityRefresh\(\)/);
  assert.doesNotMatch(applications, /modelsInitialized/);
  assert.match(applications, /onActivated\(\(\) => \{[\s\S]*?if \(!settingsLoading\.value\) void loadSettings\(\)/);
  assert.match(applications, /applicationModelIds\.value = modelIds/);
  assert.match(applications, /activeGuide\.value\.id === "claude-desktop"[\s\S]*?modelOptions\.value[\s\S]*?applicationModelIds\.value\.map/);
  assert.match(applications, /reconcileApplicationModelSelection\([\s\S]*?modelIds,[\s\S]*?defaultSelectedModels/);
  assert.match(applications, /if \(!modelIds\.includes\(modelValues\.value\[field\]\)\)/);
  assert.match(applications, /selectedModelsByApplication\.value\[currentApplication\.value\]/);
  assert.match(applications, /selectedModelByApplication\.value\[currentApplication\.value\]/);
  assert.match(restoreDefaults, /recommendClaudeCodeModel\(field, models\)/);
  assert.match(restoreDefaults, /selectedModels\.value = \[\.\.\.models\]/);
  assert.match(restoreDefaults, /selectedModel\.value = models\[0\] \?\? null/);
  assert.match(restoreDefaults, /claudeDesktopDefaults\.value/);
  assert.match(applications, /snippetDrafts|clearApplicationDrafts/);
  const settingsLoad = applications.slice(
    applications.indexOf("async function loadSettings"),
    applications.indexOf("async function copyValue"),
  );
  assert.match(settingsLoad, /reconcileConnectionDrafts\(/);
  assert.ok(
    settingsLoad.indexOf("const connection = await connectionStore.load()")
      < settingsLoad.indexOf("snippetDrafts.value = reconcileConnectionDrafts"),
  );
  assert.doesNotMatch(settingsLoad, /serviceConfig\.value\s*=/);
  assert.doesNotMatch(restoreDefaults, /loadModels|dashboardApi\./);
  assert.match(applications, /settingsStore\.putClaudeDesktop\(/);
  assert.match(applications, /const claudeDesktopModelsDirty = computed/);
  assert.match(applications, /async function saveClaudeDesktopModels\(\): Promise<boolean>/);
  assert.match(applications, /if \(!\(await saveClaudeDesktopModels\(\)\)\) return;/);
  assert.match(applications, /模型映射保存失败/);
  assert.match(applications, /@click="loadSettings\(\)"/);
  assert.match(applications, /v-model:value="selectedModels"/);
  assert.match(applications, /type="textarea"/);
  assert.match(applications, /restoreMaskedConnectionKey\(draft, guideContext\.value\.displayKey, guideContext\.value\.actualKey\)/);
  assert.doesNotMatch(applications, /Authorization: `Bearer/);
  assert.doesNotMatch(applications, /\s+tag(?:\s|>)/);
  assert.doesNotMatch(applications, /fetch\(`\$\{connectionUrls\.value\.apiBaseUrl\}/);
  assert.doesNotMatch(applications, /<n-tabs/);
  assert.doesNotMatch(applications, /<n-tab-pane/);
  assert.doesNotMatch(applications, /class="page-head"/);
  assert.doesNotMatch(applications, /class="guide-card"/);
  assert.match(applications, /maskConnectionKey\(selectedKey\.value\?\.value/);
  assert.doesNotMatch(applications, /<code>\{\{ serviceConfig\.(?:gateway_key|primary_key) \}\}<\/code>/);
  assert.match(app, /<main class="app-content">/);
  assert.doesNotMatch(app, /<n-layout-content/);
});

test("settings expose the downstream display root and bounded request timeouts", async () => {
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const api = `${await readFile(new URL("../api/dashboard.ts", import.meta.url), "utf8")}\n${await readFile(new URL("../api/dashboard-presenters.ts", import.meta.url), "utf8")}`;
  const dashboard = await readFile(new URL("./Dashboard.vue", import.meta.url), "utf8");
  const settingsMerge = await readFile(new URL("./settings-merge.ts", import.meta.url), "utf8");

  assert.match(settings, /下游访问根地址（可选）/);
  assert.match(settings, /v-model:value="config\.gateway_port"/);
  assert.match(settings, /:disabled="!loaded \|\| saving \|\| config\.gateway_port_from_env"/);
  assert.match(settings, /由环境变量 OCG_GATEWAY_PORT 管理/);
  assert.match(settings, /gateway_port_from_env: false/);
  assert.ok(settings.indexOf("t('上游地址')") < settings.indexOf('class="downstream-grid"'));
  assert.match(settings, /\.downstream-grid \{[\s\S]*grid-template-columns: repeat\(2, minmax\(0, 1fr\)\)/);
  assert.match(settings, /v-model:value="clientRootInputValue"/);
  assert.match(settings, /:readonly="config\.client_root_url_from_env"/);
  assert.match(settings, /:clearable="!config\.client_root_url_from_env && !!config\.client_root_url"/);
  assert.match(settings, /由环境变量 OCG_CLIENT_ROOT_URL 管理/);
  assert.match(settings, /v-else-if="!config\.client_root_url\.trim\(\)" class="sr-only"/);
  assert.match(settings, /\{\{ automaticClientRootFeedback \}\}/);
  assert.match(settings, /config\.client_root_url/);
  assert.match(settings, /client_root_url_from_env: false/);
  assert.match(settings, /proxy_mode: "auto"/);
  assert.match(settings, /v-model:value="config\.proxy_mode"/);
  assert.match(settings, /v-model:value="config\.proxy_url"/);
  assert.match(settings, /\{\{ proxyIntro \}\}/);
  assert.match(settings, /\{\{ proxyTestHelp \}\}/);
  assert.match(settings, /dashboardApi\.testProxy/);
  assert.match(api, /dashboardV3\.testProxy\(/);
  assert.match(settingsMerge, /"proxy_mode"[\s\S]*"proxy_url"/);
  assert.match(settings, /get: \(\) => config\.value\.client_root_url,/);
  assert.match(settings, /:placeholder="config\.client_root_url_from_env \? '' : automaticClientRootUrls\.rootUrl"/);
  assert.doesNotMatch(settings, /config\.value\.client_root_url = resolveConnectionUrls/);
  assert.match(settings, /非本机 HTTP 会明文传输 Key 与请求内容/);
  assert.match(settings, /请求超时/);
  assert.match(settings, /config\.connect_timeout_secs"\s+:disabled="!loaded"\s+:min="1"\s+:max="300"\s+:precision="0"/);
  assert.match(settings, /config\.non_stream_timeout_secs"\s+:disabled="!loaded"\s+:min="1"\s+:max="3600"\s+:precision="0"/);
  assert.match(settings, /config\.stream_idle_timeout_secs"\s+:disabled="!loaded"\s+:min="1"\s+:max="3600"\s+:precision="0"/);
  assert.match(settings, /connect_timeout_secs: 30/);
  assert.match(settings, /non_stream_timeout_secs: 900/);
  assert.match(settings, /stream_idle_timeout_secs: 300/);
  assert.match(settings, /routing_mode: "strict-priority"/);
  assert.match(settings, /conversation_sticky: false/);
  assert.match(settings, /账号路由/);
  assert.match(settings, /n-radio-group[\s\S]*config\.routing_mode/);
  assert.match(settings, /name="routing-mode"/);
  assert.match(settings, /v-for="option in routingModeOptions"/);
  assert.match(settings, /v-model:value="config\.conversation_sticky"/);
  assert.match(settings, /:disabled="!loaded \|\| saving"/);
  assert.match(settings, /严格优先级/);
  assert.match(settings, /全局粘性/);
  assert.match(settings, /轮询/);
  assert.match(settings, /config\.conversation_sticky/);
  assert.match(settings, /X-OCG-Conversation-Id/);
  assert.match(settings, /运行时路由状态已重置/);
  assert.match(settingsMerge, /"routing_mode"/);
  assert.match(settingsMerge, /"conversation_sticky"/);
  assert.match(settingsMerge, /if \(current\[key\] !== saved\[key\]\)/);
  assert.match(settings, /const routingChanged = !!savedConfig\.value/);
  assert.match(settings, /settingsStore\.putPresented\(payload\)/);
  assert.match(settings, /useSettingsStore\(\)/);
  assert.match(api, /settingsUpdateInput\(settings\)/);
  assert.match(api, /controlPlane\.runMutation\(run\)/);
  assert.match(settings, /if \(!validateTimeouts\(\)\) return/);
  assert.match(settings, /\{field\}必须为 \{min\}–\{max\} 秒的整数/);
  // Key lifecycle lives on the dedicated Keys view, not the settings form.
  assert.doesNotMatch(settings, /gatewayKeyDraft|startGatewayKeyEdit|saveGatewayKey/);
  assert.doesNotMatch(settings, /createGatewayKey|updateGatewayKey|deleteGatewayKey/);
  assert.match(settings, /v-if="settingsLoadError"[\s\S]*?@click="loadSettings"/);
  assert.match(api, /client_root_url: string/);
  assert.match(api, /client_root_url_from_env: boolean/);
  assert.match(api, /connect_timeout_secs: number/);
  assert.match(api, /non_stream_timeout_secs: number/);
  assert.match(api, /stream_idle_timeout_secs: number/);
  assert.match(api, /routing_mode: RoutingMode/);
  assert.match(api, /conversation_sticky: boolean/);
  assert.match(api, /export type RoutingMode = "strict-priority" \| "sticky-global" \| "round-robin"/);
  // The connection panel consumes the lightweight ConnectionInfo payload,
  // never the full settings shape.
  assert.match(dashboard, /useConnectionStore\(\)/);
  assert.match(dashboard, /computed\(\(\) => connectionStore\.info \?\? EMPTY_CONNECTION\)/);
  assert.doesNotMatch(dashboard, /ref<AppConfig>/);
  assert.doesNotMatch(settings, /PricingCatalog/);
  assert.match(api, /dashboardV3\.getProviderPricing\("opencode", "go"\)/);
  assert.match(api, /refreshProviderPricing: async/);
  assert.match(api, /expectedPricingRevision/);
  assert.match(api, /official_content_hash\?: string/);
  assert.match(api, /expected_official_content_hash\?: string/);
  assert.match(api, /updatePricingMultipliers: async \(expectedPricingRevision: string/);
  assert.match(api, /dashboardV3\.putPricingMultipliers\(\{[\s\S]*expectedPricingRevision/);
});

test("accounts derive quota limits from the active pricing snapshot", async () => {
  const usage = await readFile(new URL("../domain/useAccountUsage.ts", import.meta.url), "utf8");
  assert.match(usage, /quotaLimits = ref<PricingLimits \| null>\(null\)/);
  assert.match(usage, /quotaLimits\.value = \(await dashboardApi\.getPricing\(\)\)\.limits/);
  assert.doesNotMatch(usage, /window_5h:\s*12|window_week:\s*30|window_month:\s*60/);
});

test("accounts keep one enabled control instead of a duplicate status badge", async () => {
  const card = await readFile(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const template = card.slice(card.indexOf("<template>"), card.indexOf("<script setup"));

  assert.match(template, /:value="account\.enabled"/);
  assert.doesNotMatch(template, /account\.enabled \? t\("已启用"\) : t\("已禁用"\)/);
});

test("accounts confirm deletes through a dialog and keep modal state fresh", async () => {
  const accounts = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");

  assert.match(accounts, /useDialog/);
  assert.doesNotMatch(accounts, /renderAccountMenuOption|NPopconfirm/);
  assert.match(accounts, /editingAccount\.value = account/);
});

test("account form rejects whitespace-only required credentials", async () => {
  const accountForm = await readFile(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");

  assert.match(accountForm, /name:\s*\{\s*required: true,\s*whitespace: true,/);
  assert.match(accountForm, /hasField\("key"\) && !isEdit\.value/);
  assert.match(accountForm, /if \(fieldRequired\("purchase_date"\)\)/);
});

test("account notes live in the edit-account form, not on cards", async () => {
  const accounts = await readFile(new URL("./Accounts.vue", import.meta.url), "utf8");
  const accountForm = await readFile(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");
  const template = accountForm.slice(accountForm.indexOf("<template>"), accountForm.indexOf("<script setup"));
  const managedCreate = accounts.slice(
    accounts.indexOf('<n-modal\n      :show="showManagedCreate"'),
    accounts.indexOf("<ManagedAccountWizard"),
  );

  assert.doesNotMatch(accounts, /class="account-notes"|saveNotes\(|notesDrafts/);
  assert.doesNotMatch(managedCreate, /path="notes"|t\(['"]备注['"]\)/);
  assert.match(template, /v-if="hasField\('notes'\)"/);
  assert.match(accountForm, /type="textarea"/);
  assert.match(accountForm, /if \(isEdit\.value\) return t\("编辑账号"\)/);
});

test("account form keeps identity first and does not collect managed password or expiry", async () => {
  const accountForm = await readFile(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");
  const template = accountForm.slice(accountForm.indexOf("<template>"), accountForm.indexOf("<script setup"));

  assert.ok(template.indexOf('path="name"') < template.indexOf('path="username"'));
  assert.doesNotMatch(template, /path="password"|t\(['"]到期日期['"]\)/);
  assert.doesNotMatch(accountForm, /payload\.password|clearPassword/);
});

test("new account names remain explicit while the catalog controls optional login fields", async () => {
  const accountForm = await readFile(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");

  assert.match(accountForm, /@update:value="handleNameUpdate"/);
  assert.match(
    accountForm,
    /function handleNameUpdate\(value: string\) \{\s*form\.value\.name = value;/,
  );
  assert.match(accountForm, /hasField\('username'\)/);
});

test("settings expose supported Windows auto-start safely", async () => {
  const settings = await readFile(new URL("./Settings.vue", import.meta.url), "utf8");
  const api = `${await readFile(new URL("../api/dashboard.ts", import.meta.url), "utf8")}\n${await readFile(new URL("../api/dashboard-presenters.ts", import.meta.url), "utf8")}`;

  assert.match(settings, /v-if="config\.auto_start_supported"/);
  assert.match(settings, /v-if="config\.dock_visibility_supported"/);
  assert.match(settings, /@update:value="handleDockVisibilityToggle"/);
  assert.match(settings, /:value="config\.auto_start"/);
  assert.match(settings, /@update:value="handleAutoStartToggle"/);
  assert.match(settings, /:aria-label="t\('随 Windows 登录自动启动 OCG Manager'\)"/);
  assert.doesNotMatch(settings, /class="settings-subsection gateway-keys"/);
  assert.match(settings, /async function handleAutoStartToggle\(newValue: boolean\)/);
  assert.match(settings, /savedConfig\.value/);
  assert.match(settings, /if \(!loaded\.value \|\| !savedConfig\.value\) return;/);
  assert.match(settings, /const nextConfig = await settingsStore\.loadPresented\(\);/);
  assert.doesNotMatch(settings, /dashboardApi\.getConnection\(\)/);
  assert.match(settings, /savedConfig\.value = \{ \.\.\.latest \}/);
  assert.match(settings, /pendingSettingsMerge = saved \? \{ current, saved \} : null;/);
  assert.match(settings, /mergeUnsavedSettings\(latest, pending\.current, pending\.saved\)/);
  assert.match(settings, /pendingSettingsMerge = null/);
  assert.match(settings, /const generation = \+\+settingsLoadGeneration/);
  assert.match(settings, /const payload = \{ \.\.\.config\.value \}/);
  assert.match(settings, /revision: 0/);
  assert.match(settings, /reloadSettingsAfterConflict/);
  assert.match(settings, /error instanceof DashboardRequestError/);
  assert.match(settings, /error\.status !== 409/);
  assert.match(settings, /async function loadSettings\(\): Promise<boolean>/);
  assert.match(settings, /onActivated\(\(\) => \{/);
  const conflictRecovery = settings.slice(
    settings.indexOf("async function reloadSettingsAfterConflict"),
    settings.indexOf("async function saveSettings"),
  );
  assert.match(conflictRecovery, /if \(await loadSettings\(\)\) \{[\s\S]*message\.warning/);
  assert.match(conflictRecovery, /else \{[\s\S]*message\.error/);
  assert.doesNotMatch(
    conflictRecovery,
    /updateSettings/,
  );
  assert.match(api, /auto_start_supported: boolean/);
  assert.match(api, /dock_visibility_supported: boolean/);
  assert.match(api, /controlPlane\.runMutation\(run\)/);
});
