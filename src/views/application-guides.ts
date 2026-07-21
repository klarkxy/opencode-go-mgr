import { t } from "../i18n/index.ts";
import type { MessageKey } from "../i18n/index.ts";

export interface GuideContext {
  rootUrl: string;
  apiBaseUrl: string;
  chatCompletionsUrl: string;
  responsesUrl: string;
  messagesUrl: string;
  displayKey: string;
  actualKey: string;
  modelId: string;
  modelIds: readonly string[];
  modelValues: Readonly<Record<string, string>>;
  iconUrl: string;
}

export interface GuideSnippet {
  label: string;
  language: string;
  display: string;
  copy: string;
}

export interface GuideAction {
  id: string;
  kind: "copy" | "launch";
  label: MessageKey;
  build: (context: GuideContext) => string;
}

export interface ApplicationGuide {
  id: string;
  name: string;
  category: MessageKey;
  protocol: string;
  endpointKind: "messages" | "responses" | "chat" | "gemini";
  officialUrl: string;
  badge?: string;
  summary: MessageKey;
  steps: readonly MessageKey[];
  notes: readonly MessageKey[];
  snippets: (context: GuideContext) => GuideSnippet[];
  modelFields?: readonly string[];
  multipleModels?: boolean;
  quickActions?: readonly GuideAction[];
}

export interface ApplicationModelSelection {
  selectedModels: string[];
  selectedModel: string | null;
}

export function reconcileApplicationModelSelection(
  currentModels: readonly string[] | undefined,
  currentModel: string | null | undefined,
  availableModels: readonly string[],
  defaultModels: readonly string[],
  multipleModels: boolean,
): ApplicationModelSelection {
  const available = new Set(availableModels);
  if (!multipleModels) {
    return {
      selectedModels: [],
      selectedModel: currentModel && available.has(currentModel)
        ? currentModel
        : availableModels[0] ?? null,
    };
  }

  const uniqueValid = (models: readonly string[]) => [...new Set(
    models.filter((model) => available.has(model)),
  )];
  const preservedModels = uniqueValid(currentModels ?? []);
  const selectedModels = preservedModels.length
    ? preservedModels
    : uniqueValid(defaultModels);
  return {
    selectedModels,
    selectedModel: currentModel && selectedModels.includes(currentModel)
      ? currentModel
      : selectedModels[0] ?? null,
  };
}

function models(context: GuideContext): readonly string[] {
  return context.modelIds.length ? context.modelIds : [context.modelId];
}

function keyedSnippet(
  context: GuideContext,
  label: string,
  language: string,
  render: (key: string) => string,
): GuideSnippet {
  return {
    label,
    language,
    display: render(context.displayKey),
    copy: render(context.actualKey),
  };
}

function encodePayload(payload: unknown): string {
  const bytes = new TextEncoder().encode(JSON.stringify(payload));
  const base64 = btoa(String.fromCharCode(...bytes));
  return encodeURIComponent(base64);
}

export function buildChatboxConfig(context: GuideContext) {
  return {
    id: `ocg-manager-${encodeURIComponent(context.rootUrl)}`,
    name: "OCG Manager",
    type: "openai" as const,
    iconUrl: context.iconUrl,
    urls: { website: `${context.rootUrl}/dashboard/` },
    settings: {
      apiHost: context.rootUrl,
      apiPath: "/v1/chat/completions",
      apiKey: context.actualKey,
      models: models(context).map((modelId) => ({
        modelId,
        nickname: modelId,
        type: "chat" as const,
        capabilities: ["tool_use"] as const,
      })),
    },
  };
}

export function buildChatboxUrl(context: GuideContext): string {
  return `chatbox://provider/import?config=${encodePayload(buildChatboxConfig(context))}`;
}

const CLAUDE_CODE_MODEL_PREFERENCES: Readonly<Record<string, readonly string[]>> = {
  ANTHROPIC_MODEL: ["qwen3.7-plus", "minimax-m3", "kimi-k2.7-code", "glm-5.2"],
  ANTHROPIC_DEFAULT_FABLE_MODEL: ["qwen3.7-max", "glm-5.2", "kimi-k2.7-code", "deepseek-v4-pro"],
  ANTHROPIC_DEFAULT_HAIKU_MODEL: ["deepseek-v4-flash", "minimax-m3", "mimo-v2.5"],
  ANTHROPIC_DEFAULT_SONNET_MODEL: ["qwen3.7-plus", "minimax-m3", "kimi-k2.7-code", "glm-5.2"],
  ANTHROPIC_DEFAULT_OPUS_MODEL: ["glm-5.2", "qwen3.7-max", "kimi-k2.7-code", "deepseek-v4-pro"],
  CLAUDE_CODE_SUBAGENT_MODEL: ["minimax-m3", "qwen3.7-plus", "deepseek-v4-flash", "kimi-k2.7-code"],
  ANTHROPIC_CUSTOM_MODEL_OPTION: ["kimi-k2.7-code", "glm-5.2", "qwen3.7-max", "deepseek-v4-pro"],
};

export function recommendClaudeCodeModel(field: string, availableModels: readonly string[]): string {
  return CLAUDE_CODE_MODEL_PREFERENCES[field]
    ?.find((model) => availableModels.includes(model))
    ?? availableModels[0]
    ?? "";
}

type ApplicationModelInput = "text" | "image" | "audio" | "video";
type ReasoningEffort = "low" | "medium" | "high" | "max";
type PiThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
type PiCompatValue = string | boolean;

// A visible high state keeps Pi's reasoning UI honest without sending an unsupported effort value.
const PI_HIGH_ONLY = {
  off: null,
  minimal: null,
  low: null,
  medium: null,
  xhigh: null,
  max: null,
} as const;

const PI_NO_REASONING_EFFORT = { supportsReasoningEffort: false } as const;
// OCG's Chat -> Messages bridge understands low/medium/high; map Pi's extra minimal level explicitly.
const PI_MINIMAL_TO_LOW = { minimal: "low" } as const;

export interface ApplicationModelMetadata {
  contextWindow: number;
  maxOutputTokens: number;
  input: readonly ApplicationModelInput[];
  /** A narrower client-facing set when OCG's protocol conversion cannot carry every native modality. */
  ocgInput?: readonly ApplicationModelInput[];
  reasoning: boolean;
  alwaysThinking?: boolean;
  toolUse: boolean;
  efforts?: readonly ReasoningEffort[];
  defaultEffort?: ReasoningEffort;
  piThinkingLevelMap?: Readonly<Partial<Record<PiThinkingLevel, string | null>>>;
  piCompat?: Readonly<Record<string, PiCompatValue>>;
}

// Effective OpenCode Go limits and capabilities, verified 2026-07-21; not generic vendor defaults.
// Source of truth: https://github.com/anomalyco/models.dev/tree/dev/providers/opencode-go/models
// Keep this exhaustive for every model that application_models can return. Unknown IDs must fail
// visibly instead of inheriting Pi/Kimi Code's misleading 128K defaults.
export const APPLICATION_MODEL_METADATA: Readonly<Record<string, ApplicationModelMetadata>> = {
  "grok-4.5": {
    contextWindow: 500_000,
    maxOutputTokens: 500_000,
    input: ["text", "image"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
  },
  "glm-5.2": {
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    efforts: ["high", "max"],
    defaultEffort: "max",
    piThinkingLevelMap: {
      off: null,
      minimal: null,
      low: null,
      medium: null,
      high: "high",
      xhigh: null,
      max: "max",
    },
  },
  "glm-5.1": {
    contextWindow: 202_752,
    maxOutputTokens: 32_768,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: PI_HIGH_ONLY,
    piCompat: PI_NO_REASONING_EFFORT,
  },
  "kimi-k3": {
    contextWindow: 1_048_576,
    maxOutputTokens: 131_072,
    input: ["text", "image", "video"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    efforts: ["max"],
    defaultEffort: "max",
  },
  "kimi-k2.7-code": {
    contextWindow: 262_144,
    maxOutputTokens: 262_144,
    input: ["text", "image", "video"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    piThinkingLevelMap: PI_HIGH_ONLY,
    piCompat: PI_NO_REASONING_EFFORT,
  },
  "kimi-k2.6": {
    contextWindow: 262_144,
    maxOutputTokens: 65_536,
    input: ["text", "image", "video"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: {
      minimal: null,
      low: null,
      medium: null,
    },
    piCompat: {
      thinkingFormat: "deepseek",
      supportsReasoningEffort: false,
      supportsLongCacheRetention: false,
    },
  },
  "mimo-v2.5": {
    contextWindow: 1_000_000,
    maxOutputTokens: 128_000,
    input: ["text", "image", "audio", "video"],
    reasoning: true,
    toolUse: true,
  },
  "mimo-v2.5-pro": {
    contextWindow: 1_048_576,
    maxOutputTokens: 128_000,
    input: ["text"],
    reasoning: true,
    toolUse: true,
  },
  "minimax-m3": {
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    input: ["text", "image", "video"],
    ocgInput: ["text", "image"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: PI_MINIMAL_TO_LOW,
  },
  "minimax-m2.7": {
    contextWindow: 204_800,
    maxOutputTokens: 131_072,
    input: ["text"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    piThinkingLevelMap: PI_HIGH_ONLY,
    piCompat: PI_NO_REASONING_EFFORT,
  },
  "minimax-m2.7-highspeed": {
    // OCG-supported faster alias; MiniMax documents capability parity with minimax-m2.7.
    contextWindow: 204_800,
    maxOutputTokens: 131_072,
    input: ["text"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    piThinkingLevelMap: PI_HIGH_ONLY,
    piCompat: PI_NO_REASONING_EFFORT,
  },
  "minimax-m2.5": {
    contextWindow: 204_800,
    maxOutputTokens: 65_536,
    input: ["text"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    piThinkingLevelMap: PI_HIGH_ONLY,
    piCompat: PI_NO_REASONING_EFFORT,
  },
  "minimax-m2.5-highspeed": {
    // OCG-supported faster alias; MiniMax documents capability parity with minimax-m2.5.
    contextWindow: 204_800,
    maxOutputTokens: 65_536,
    input: ["text"],
    reasoning: true,
    alwaysThinking: true,
    toolUse: true,
    piThinkingLevelMap: PI_HIGH_ONLY,
    piCompat: PI_NO_REASONING_EFFORT,
  },
  "qwen3.7-max": {
    contextWindow: 1_000_000,
    maxOutputTokens: 65_536,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: PI_MINIMAL_TO_LOW,
  },
  "qwen3.7-plus": {
    contextWindow: 1_000_000,
    maxOutputTokens: 65_536,
    input: ["text", "image", "video"],
    ocgInput: ["text", "image"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: PI_MINIMAL_TO_LOW,
  },
  "qwen3.6-plus": {
    contextWindow: 1_000_000,
    maxOutputTokens: 65_536,
    input: ["text", "image", "video"],
    ocgInput: ["text", "image"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: PI_MINIMAL_TO_LOW,
  },
  "deepseek-v4-pro": {
    contextWindow: 1_000_000,
    maxOutputTokens: 384_000,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    efforts: ["high", "max"],
    defaultEffort: "high",
    piCompat: {
      requiresReasoningContentOnAssistantMessages: true,
      thinkingFormat: "deepseek",
    },
  },
  "deepseek-v4-flash": {
    contextWindow: 1_000_000,
    maxOutputTokens: 384_000,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    efforts: ["high", "max"],
    defaultEffort: "high",
    piCompat: {
      requiresReasoningContentOnAssistantMessages: true,
      thinkingFormat: "deepseek",
    },
  },
};

function applicationModelMetadata(modelId: string): ApplicationModelMetadata {
  const metadata = APPLICATION_MODEL_METADATA[modelId];
  if (!metadata) {
    throw new Error(`Missing verified application model metadata for ${JSON.stringify(modelId)}`);
  }
  return metadata;
}

function piThinkingLevelMap(metadata: ApplicationModelMetadata): Readonly<Record<string, string | null>> | undefined {
  if (metadata.piThinkingLevelMap) return metadata.piThinkingLevelMap;
  const mapping: Record<string, string | null> = {};
  if (metadata.alwaysThinking) mapping.off = null;
  if (metadata.efforts) {
    for (const level of ["minimal", "low", "medium", "high", "xhigh", "max"] as const) {
      mapping[level] = metadata.efforts.includes(level as ReasoningEffort) ? level : null;
    }
  }
  return Object.keys(mapping).length ? mapping : undefined;
}

function piModelConfig(modelId: string) {
  const metadata = applicationModelMetadata(modelId);
  const effectiveInput = metadata.ocgInput ?? metadata.input;
  const thinkingLevelMap = piThinkingLevelMap(metadata);
  return {
    id: modelId,
    reasoning: metadata.reasoning,
    input: effectiveInput.includes("image") ? ["text", "image"] : ["text"],
    contextWindow: metadata.contextWindow,
    maxTokens: metadata.maxOutputTokens,
    ...(thinkingLevelMap ? { thinkingLevelMap } : {}),
    ...(metadata.piCompat ? { compat: metadata.piCompat } : {}),
  };
}

const PI_PROVIDER_COMPAT = {
  supportsStore: false,
  supportsDeveloperRole: false,
  maxTokensField: "max_tokens",
} as const;

function kimiCodeCapabilities(metadata: ApplicationModelMetadata): string[] {
  const effectiveInput = metadata.ocgInput ?? metadata.input;
  return [
    ...(metadata.reasoning ? ["thinking"] : []),
    ...(metadata.alwaysThinking ? ["always_thinking"] : []),
    ...(effectiveInput.includes("image") ? ["image_in"] : []),
    ...(effectiveInput.includes("video") ? ["video_in"] : []),
    ...(effectiveInput.includes("audio") ? ["audio_in"] : []),
    ...(metadata.toolUse ? ["tool_use"] : []),
  ];
}

function kimiCodeModelTable(modelId: string): string {
  const metadata = applicationModelMetadata(modelId);
  const alias = `ocg/${modelId}`;
  const effortLines = metadata.efforts
    ? `\nsupport_efforts = ${JSON.stringify(metadata.efforts)}`
      + (metadata.defaultEffort ? `\ndefault_effort = ${JSON.stringify(metadata.defaultEffort)}` : "")
    : "";
  return `[models.${JSON.stringify(alias)}]\nprovider = "ocg"\nmodel = ${JSON.stringify(modelId)}\nmax_context_size = ${metadata.contextWindow}\ncapabilities = ${JSON.stringify(kimiCodeCapabilities(metadata))}\ndisplay_name = ${JSON.stringify(`${modelId} (OCG Manager)`)}${effortLines}`;
}

const VSCODE_MODEL_CONTEXT_WINDOWS = Object.fromEntries(
  Object.entries(APPLICATION_MODEL_METADATA).map(([modelId, metadata]) => [modelId, metadata.contextWindow]),
) as Readonly<Record<string, number>>;

function vscodeTokenLimits(modelId: string) {
  const contextWindow = VSCODE_MODEL_CONTEXT_WINDOWS[modelId];
  // ponytail: unknown future models keep conservative limits until their real window is added above.
  if (!contextWindow) return { maxInputTokens: 32_768, maxOutputTokens: 8_192 };
  const maxOutputTokens = modelId === "glm-5.1" ? 32_768 : 65_536;
  return { maxInputTokens: contextWindow - maxOutputTokens, maxOutputTokens };
}

export const APPLICATION_GUIDES = [
  {
    id: "claude-code",
    name: "Claude Code",
    category: "Claude 兼容",
    protocol: "Anthropic Messages",
    endpointKind: "messages",
    officialUrl: "https://code.claude.com/docs/en/llm-gateway-connect",
    summary: "通过 Anthropic 兼容入口连接 OCG Manager，地址使用不带 /v1 的根地址。",
    steps: [
      "打开用户级 ~/.claude/settings.json，将下面的环境变量和模型配置合并进去。",
      "确认 ANTHROPIC_BASE_URL 使用下方根地址，ANTHROPIC_AUTH_TOKEN 使用 Key。",
      "启动 Claude Code 并发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "Claude Code 使用 Anthropic Messages 协议，因此不要给 ANTHROPIC_BASE_URL 追加 /v1。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    modelFields: [
      "ANTHROPIC_MODEL",
      "ANTHROPIC_DEFAULT_FABLE_MODEL",
      "ANTHROPIC_DEFAULT_HAIKU_MODEL",
      "ANTHROPIC_DEFAULT_SONNET_MODEL",
      "ANTHROPIC_DEFAULT_OPUS_MODEL",
      "CLAUDE_CODE_SUBAGENT_MODEL",
      "ANTHROPIC_CUSTOM_MODEL_OPTION",
    ],
    snippets: (context) => [
      keyedSnippet(context, "~/.claude/settings.json", "json", (key) =>
        JSON.stringify(
          {
            env: {
              ANTHROPIC_BASE_URL: context.rootUrl,
              ANTHROPIC_AUTH_TOKEN: key,
              CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY: "1",
              ANTHROPIC_MODEL: context.modelValues.ANTHROPIC_MODEL || context.modelId,
              ANTHROPIC_DEFAULT_FABLE_MODEL: context.modelValues.ANTHROPIC_DEFAULT_FABLE_MODEL || context.modelId,
              ANTHROPIC_DEFAULT_HAIKU_MODEL: context.modelValues.ANTHROPIC_DEFAULT_HAIKU_MODEL || context.modelId,
              ANTHROPIC_DEFAULT_SONNET_MODEL: context.modelValues.ANTHROPIC_DEFAULT_SONNET_MODEL || context.modelId,
              ANTHROPIC_DEFAULT_OPUS_MODEL: context.modelValues.ANTHROPIC_DEFAULT_OPUS_MODEL || context.modelId,
              CLAUDE_CODE_SUBAGENT_MODEL: context.modelValues.CLAUDE_CODE_SUBAGENT_MODEL || context.modelId,
              ANTHROPIC_CUSTOM_MODEL_OPTION: context.modelValues.ANTHROPIC_CUSTOM_MODEL_OPTION || context.modelId,
            },
            model: context.modelValues.ANTHROPIC_MODEL || context.modelId,
          },
          null,
          2,
        ),
      ),
    ],
  },
  {
    id: "claude-desktop",
    name: "Claude Desktop",
    category: "Claude 兼容",
    protocol: "Anthropic Messages",
    endpointKind: "messages",
    officialUrl: "https://github.com/farion1231/cc-switch/blob/main/docs/user-manual/en/2-providers/2.6-claude-desktop.md",
    summary: "通过 Anthropic 兼容入口连接 OCG Manager，地址使用不带 /v1 的根地址。",
    steps: [
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    modelFields: ["sonnet", "opus", "haiku"],
    snippets: (context) => [
      keyedSnippet(context, "Claude Desktop 3P profile", "json", (key) =>
        JSON.stringify(
          {
            inferenceProvider: "gateway",
            inferenceGatewayBaseUrl: `${context.rootUrl}/claude-desktop`,
            inferenceGatewayAuthScheme: "bearer",
            inferenceGatewayApiKey: key,
          },
          null,
          2,
        ),
      ),
    ],
  },
  {
    id: "codex",
    name: "Codex",
    category: "OpenAI 兼容",
    protocol: "OpenAI Responses",
    endpointKind: "responses",
    officialUrl: "https://developers.openai.com/codex/config-reference/",
    badge: "Responses",
    summary: "注册 OCG Manager 为 Codex 自定义模型提供商，通过 Responses 接口调用。",
    steps: [
      "把模型与 provider 配置写入用户级 ~/.codex/config.toml。",
      "在启动 Codex 的同一终端设置 OCG_API_KEY 环境变量。",
      "启动 Codex 并发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "OCG Manager 当前提供无状态 Responses 转发，不要依赖 previous_response_id 延续服务端状态。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    modelFields: ["model", "review_model"],
    snippets: (context) => [
      {
        label: "~/.codex/config.toml",
        language: "toml",
        display: `model = ${JSON.stringify(context.modelValues.model || context.modelId)}\nreview_model = ${JSON.stringify(context.modelValues.review_model || context.modelId)}\nmodel_provider = "ocg"\n\n[model_providers.ocg]\nname = "OCG Manager"\nbase_url = "${context.apiBaseUrl}"\nenv_key = "OCG_API_KEY"\nwire_api = "responses"`,
        copy: `model = ${JSON.stringify(context.modelValues.model || context.modelId)}\nreview_model = ${JSON.stringify(context.modelValues.review_model || context.modelId)}\nmodel_provider = "ocg"\n\n[model_providers.ocg]\nname = "OCG Manager"\nbase_url = "${context.apiBaseUrl}"\nenv_key = "OCG_API_KEY"\nwire_api = "responses"`,
      },
      keyedSnippet(
        context,
        t("当前 PowerShell 会话"),
        "powershell",
        (key) => `$env:OCG_API_KEY = ${JSON.stringify(key)}`,
      ),
      keyedSnippet(
        context,
        "macOS / Linux shell",
        "bash",
        (key) => `export OCG_API_KEY=${JSON.stringify(key)}`,
      ),
    ],
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    category: "Gemini",
    protocol: "Gemini generateContent",
    endpointKind: "gemini",
    officialUrl: "https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md",
    summary: "填写下方 Base URL、Key 和模型 ID。",
    steps: [
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
      "Gemini CLI 的远程 Base URL 必须使用 HTTPS；仅 localhost、127.0.0.1 和 [::1] 可使用 HTTP。",
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        "~/.gemini/.env",
        "dotenv",
        (key) => `GEMINI_API_KEY=${JSON.stringify(key)}\nGOOGLE_GEMINI_BASE_URL=${context.rootUrl}\nGOOGLE_GENAI_API_VERSION=v1beta`,
      ),
      {
        label: "~/.gemini/settings.json",
        language: "json",
        display: JSON.stringify(
          {
            $schema: "https://raw.githubusercontent.com/google-gemini/gemini-cli/main/schemas/settings.schema.json",
            model: { name: context.modelId },
            modelConfigs: {
              customOverrides: [
                {
                  match: { overrideScope: "core" },
                  modelConfig: { model: context.modelId },
                },
              ],
            },
            agents: {
              overrides: Object.fromEntries(
                ["codebase_investigator", "cli_help", "generalist", "browser_agent"].map((agent) => [
                  agent,
                  { modelConfig: { model: context.modelId } },
                ]),
              ),
            },
          },
          null,
          2,
        ),
        copy: JSON.stringify(
          {
            $schema: "https://raw.githubusercontent.com/google-gemini/gemini-cli/main/schemas/settings.schema.json",
            model: { name: context.modelId },
            modelConfigs: {
              customOverrides: [
                {
                  match: { overrideScope: "core" },
                  modelConfig: { model: context.modelId },
                },
              ],
            },
            agents: {
              overrides: Object.fromEntries(
                ["codebase_investigator", "cli_help", "generalist", "browser_agent"].map((agent) => [
                  agent,
                  { modelConfig: { model: context.modelId } },
                ]),
              ),
            },
          },
          null,
          2,
        ),
      },
    ],
  },
  {
    id: "pi",
    name: "Pi",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://pi.dev/docs/latest/models",
    summary: "在 models.json 中注册 OCG Manager，通过 Chat Completions 使用 Pi Agent。",
    steps: [
      "把下面的 provider 配置合并到用户级 ~/.pi/agent/models.json。",
      "在启动 Pi 的同一终端设置 OCG_API_KEY 环境变量。",
      "启动 Pi 并发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      {
        label: "~/.pi/agent/models.json",
        language: "json",
        display: JSON.stringify({
          providers: {
            ocg: {
              baseUrl: context.apiBaseUrl,
              api: "openai-completions",
              apiKey: "$OCG_API_KEY",
              compat: PI_PROVIDER_COMPAT,
              models: models(context).filter(Boolean).map(piModelConfig),
            },
          },
        }, null, 2),
        copy: JSON.stringify({
          providers: {
            ocg: {
              baseUrl: context.apiBaseUrl,
              api: "openai-completions",
              apiKey: "$OCG_API_KEY",
              compat: PI_PROVIDER_COMPAT,
              models: models(context).filter(Boolean).map(piModelConfig),
            },
          },
        }, null, 2),
      },
      keyedSnippet(
        context,
        t("当前 PowerShell 会话"),
        "powershell",
        (key) => `$env:OCG_API_KEY = ${JSON.stringify(key)}`,
      ),
      keyedSnippet(
        context,
        "macOS / Linux shell",
        "bash",
        (key) => `export OCG_API_KEY=${JSON.stringify(key)}`,
      ),
    ],
    multipleModels: true,
  },
  {
    id: "kimi-code",
    name: "Kimi Code CLI",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files",
    summary: "选择 OpenAI Compatible provider，将对话请求转发到 OCG Manager。",
    steps: [
      "把下面的 provider 与 model 配置合并到用户级 ~/.kimi-code/config.toml。",
      "启动 Kimi Code CLI 并发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "Kimi CLI 已迁移到 Kimi Code CLI；新接入使用 ~/.kimi-code 而不是旧版 ~/.kimi。",
      "Kimi Code CLI 会把 api_key 明文保存在 config.toml；请限制配置目录权限。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(context, "~/.kimi-code/config.toml", "toml", (key) => {
        const modelIds = models(context).filter(Boolean);
        const defaultModel = modelIds[0] ? `default_model = ${JSON.stringify(`ocg/${modelIds[0]}`)}\n` : "";
        const modelTables = modelIds.map(kimiCodeModelTable).join("\n\n");
        return `${defaultModel}default_permission_mode = "manual"\n\n[providers.ocg]\ntype = "openai"\nbase_url = ${JSON.stringify(context.apiBaseUrl)}\napi_key = ${JSON.stringify(key)}${modelTables ? `\n\n${modelTables}` : ""}`;
      }),
    ],
    multipleModels: true,
  },
  {
    id: "opencode",
    name: "OpenCode",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://opencode.ai/docs/providers/",
    summary: "使用 OpenAI Compatible AI SDK provider，将 OCG Manager 注册为自定义服务商。",
    steps: [
      "把下面的 provider 配置合并到项目或用户级 opencode.json。",
      "在启动 OpenCode 的同一终端设置 OCG_API_KEY 环境变量。",
      "在 OpenCode 中发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(context, "opencode.json", "json", () =>
        JSON.stringify(
          {
            $schema: "https://opencode.ai/config.json",
            provider: {
              ocg: {
                npm: "@ai-sdk/openai-compatible",
                name: "OCG Manager",
                options: { baseURL: context.apiBaseUrl, apiKey: "{env:OCG_API_KEY}" },
                models: Object.fromEntries(models(context).map((modelId) => [
                  modelId,
                  { name: modelId, reasoning: true },
                ])),
              },
            },
            model: `ocg/${context.modelId}`,
          },
          null,
          2,
        ),
      ),
      keyedSnippet(
        context,
        t("当前 PowerShell 会话"),
        "powershell",
        (key) => `$env:OCG_API_KEY = ${JSON.stringify(key)}`,
      ),
      keyedSnippet(
        context,
        "macOS / Linux shell",
        "bash",
        (key) => `export OCG_API_KEY=${JSON.stringify(key)}`,
      ),
    ],
    multipleModels: true,
  },
  {
    id: "openclaw",
    name: "OpenClaw",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://docs.openclaw.ai/concepts/model-providers",
    summary: "选择 OpenAI Compatible provider，将对话请求转发到 OCG Manager。",
    steps: [
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => {
      const config = JSON.stringify(
        {
          models: {
            mode: "merge",
            providers: {
              ocg: {
                baseUrl: context.apiBaseUrl,
                apiKey: "${OCG_API_KEY}",
                api: "openai-completions",
                models: models(context).map((modelId) => ({ id: modelId, name: modelId })),
              },
            },
          },
          agents: {
            defaults: {
              model: { primary: `ocg/${context.modelId}` },
              models: Object.fromEntries(models(context).map((modelId) => [`ocg/${modelId}`, {}])),
            },
          },
        },
        null,
        2,
      );
      return [
        { label: "~/.openclaw/openclaw.json", language: "json5", display: config, copy: config },
        keyedSnippet(
          context,
          "~/.openclaw/.env",
          "dotenv",
          (key) => `OCG_API_KEY=${JSON.stringify(key)}`,
        ),
      ];
    },
    multipleModels: true,
  },
  {
    id: "hermes",
    name: "Hermes",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://hermes-agent.nousresearch.com/docs/integrations/providers",
    summary: "选择 OpenAI Compatible provider，将对话请求转发到 OCG Manager。",
    steps: [
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      {
        label: "~/.hermes/config.yaml",
        language: "yaml",
        display: `custom_providers:\n  - name: ocg\n    base_url: ${JSON.stringify(context.apiBaseUrl)}\n    key_env: OCG_API_KEY\n    api_mode: chat_completions\n    models:\n${models(context).map((modelId) => `      ${JSON.stringify(modelId)}: {}`).join("\n")}\n\nmodel:\n  default: ${JSON.stringify(context.modelId)}\n  provider: custom:ocg`,
        copy: `custom_providers:\n  - name: ocg\n    base_url: ${JSON.stringify(context.apiBaseUrl)}\n    key_env: OCG_API_KEY\n    api_mode: chat_completions\n    models:\n${models(context).map((modelId) => `      ${JSON.stringify(modelId)}: {}`).join("\n")}\n\nmodel:\n  default: ${JSON.stringify(context.modelId)}\n  provider: custom:ocg`,
      },
      keyedSnippet(context, "~/.hermes/.env", "dotenv", (key) => `OCG_API_KEY=${JSON.stringify(key)}`),
    ],
    multipleModels: true,
  },
  {
    id: "cherry-studio",
    name: "Cherry Studio",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://docs.cherry-ai.com/docs/en-us/pre-basic/settings/providers",
    summary: "在服务商设置中新增 OpenAI 类型的自定义服务商，并手工添加可用模型。",
    steps: [
      "进入设置 → 模型服务，新增 OpenAI 类型的自定义服务商。",
      "填写下方 Base URL、Key 和模型 ID。",
      "执行连接检查或发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: ["API 地址使用不带 /v1 的根地址，由 Cherry Studio 补全 OpenAI 请求路径。"],
    snippets: (context) => [
      keyedSnippet(
        context,
        t("服务商参数"),
        "text",
        (key) =>
          t("服务商类型: OpenAI\nAPI 地址: {url}\nAPI Key: {key}\n模型 ID: {model}", {
            url: context.rootUrl,
            key,
            model: models(context).join(", "),
          }),
      ),
    ],
    multipleModels: true,
  },
  {
    id: "vscode-copilot",
    name: "VS Code Copilot Chat",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://code.visualstudio.com/docs/agent-customization/language-models",
    badge: "BYOK",
    summary: "在 Copilot Chat 的自带密钥模型设置中添加 Custom Endpoint 完整端点。",
    steps: [
      "在 Copilot Chat 的模型管理中选择 Custom Endpoint，并将 API 类型设为 Chat Completions。",
      "填写下方完整 Chat Completions Endpoint、Key 和模型 ID。",
      "在 Chat 中选择该模型并发送测试消息，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: [
      "BYOK 只影响支持自带密钥的聊天模型，不接管 Copilot 行内补全、embedding 等能力。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(context, "chatLanguageModels.json", "json", (key) =>
        JSON.stringify(
          [{
            name: "OCG Manager",
            vendor: "customendpoint",
            apiKey: key,
            apiType: "chat-completions",
            models: models(context).map((modelId) => ({
              id: modelId,
              name: modelId,
              url: context.chatCompletionsUrl,
              toolCalling: true,
              vision: false,
              ...vscodeTokenLimits(modelId),
            })),
          }],
          null,
          2,
        ),
      ),
    ],
    multipleModels: true,
  },
  {
    id: "cline",
    name: "Cline",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://docs.cline.bot/provider-config/openai-compatible",
    summary: "选择 OpenAI Compatible provider，直接填写 OCG Manager 的 API Base URL。",
    steps: [
      "打开 Cline 设置，将 API Provider 选择为 OpenAI Compatible。",
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: ["模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。"],
    snippets: (context) => [
      keyedSnippet(
        context,
        t("Provider 参数"),
        "text",
        (key) => `Base URL: ${context.apiBaseUrl}\nAPI Key: ${key}\nModel ID: ${context.modelId}`,
      ),
    ],
  },
  {
    id: "roo-code",
    name: "Roo Code",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://roocodeinc.github.io/Roo-Code/providers/openai-compatible/",
    summary: "选择 OpenAI Compatible provider，将对话请求转发到 OCG Manager。",
    steps: [
      "打开 Roo Code 配置，将 API Provider 选择为 OpenAI Compatible。",
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: ["Roo Code 仅支持原生工具调用；所选模型不支持 tools 时无法使用 Agent 模式。"],
    snippets: (context) => [
      keyedSnippet(
        context,
        t("Provider 参数"),
        "text",
        (key) => `Base URL: ${context.apiBaseUrl}\nAPI Key: ${key}\nModel ID: ${context.modelId}`,
      ),
    ],
  },
  {
    id: "continue",
    name: "Continue",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://docs.continue.dev/customize/model-providers/top-level/openai",
    summary: "在 Continue YAML 配置中添加 OpenAI provider，并明确关闭 Responses API。",
    steps: [
      "打开 Continue 用户级 YAML 配置，将下面的模型项合并到 models。",
      "保持 provider 为 openai、apiBase 使用 /v1 地址、useResponsesApi 为 false。",
      "选择 OCG Manager 模型发送测试消息，再到请求日志确认成功记录。",
    ],
    notes: [
      "useResponsesApi: false 用于明确走 Chat Completions 兼容路径。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        "Continue YAML",
        "yaml",
        (key) =>
          `name: OCG Manager\nversion: 1.0.0\nschema: v1\nmodels:\n${models(context).map((modelId) => `  - name: ${JSON.stringify(`${modelId} (OCG)`)}\n    provider: openai\n    model: ${JSON.stringify(modelId)}\n    apiBase: ${JSON.stringify(context.apiBaseUrl)}\n    apiKey: ${JSON.stringify(key)}\n    useResponsesApi: false\n    capabilities:\n      - tool_use`).join("\n")}`,
      ),
    ],
    multipleModels: true,
  },
  {
    id: "chatbox",
    name: "Chatbox",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://docs.chatboxai.app/en/guides/providers/import-config",
    summary: "新增 OpenAI API 类型提供商，API Host 使用 OCG Manager 根地址。",
    steps: [
      "打开设置 → 模型提供方，选择 OpenAI API 或兼容提供方。",
      "填写下方 API Host、Key 和模型 ID，保留默认的 /v1/chat/completions 路径。",
      "发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。",
    ],
    notes: ["API Host 使用不带 /v1 的根地址，避免形成重复路径。"],
    snippets: (context) => [
      keyedSnippet(
        context,
        t("Provider 参数"),
        "text",
        (key) => `API Host: ${context.rootUrl}\nAPI Key: ${key}\nModel IDs: ${models(context).join(", ")}`,
      ),
    ],
    quickActions: [
      {
        id: "chatbox-copy",
        kind: "copy",
        label: "复制配置",
        build: (context) => JSON.stringify(buildChatboxConfig(context), null, 2),
      },
      {
        id: "chatbox-import",
        kind: "launch",
        label: "一键导入",
        build: buildChatboxUrl,
      },
    ],
    multipleModels: true,
  },
] as const satisfies readonly ApplicationGuide[];

export type ApplicationId = (typeof APPLICATION_GUIDES)[number]["id"];

export function isApplicationId(value: string | null | undefined): value is ApplicationId {
  return typeof value === "string" && APPLICATION_GUIDES.some((guide) => guide.id === value);
}
