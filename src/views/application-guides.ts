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
  /** All Applications-selectable model IDs (used by Codex catalog, etc.). */
  availableModelIds: readonly string[];
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
  popular?: boolean;
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

function modelCapabilities(modelId: string): ApplicationModelMetadata | undefined {
  return APPLICATION_MODEL_METADATA[modelId];
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
    name: "Open Console Gateway",
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
        capabilities: [
          ...(modelCapabilities(modelId)?.reasoning ? ["reasoning" as const] : []),
          ...((modelCapabilities(modelId)?.ocgInput ?? modelCapabilities(modelId)?.input)?.includes("image")
            ? ["vision" as const]
            : []),
          ...(modelCapabilities(modelId)?.toolUse === false ? [] : ["tool_use" as const]),
        ],
      })),
    },
  };
}

export function buildChatboxUrl(context: GuideContext): string {
  return `chatbox://provider/import?config=${encodePayload(buildChatboxConfig(context))}`;
}

// Prefer models whose supported set includes Messages so Claude Code can
// passthrough. Chat-only IDs still work via Gateway conversion, but they are
// not first picks.
const CLAUDE_CODE_MODEL_PREFERENCES: Readonly<Record<string, readonly string[]>> = {
  ANTHROPIC_MODEL: ["qwen3.7-plus", "minimax-m3", "kimi-k3", "glm-5.2"],
  ANTHROPIC_DEFAULT_FABLE_MODEL: ["qwen3.7-max", "glm-5.2", "kimi-k3", "deepseek-v4-pro"],
  ANTHROPIC_DEFAULT_HAIKU_MODEL: ["deepseek-v4-flash", "minimax-m3", "glm-5.1", "kimi-k3"],
  ANTHROPIC_DEFAULT_SONNET_MODEL: ["qwen3.7-plus", "minimax-m3", "kimi-k3", "glm-5.2"],
  ANTHROPIC_DEFAULT_OPUS_MODEL: ["glm-5.2", "qwen3.7-max", "kimi-k3", "deepseek-v4-pro"],
  CLAUDE_CODE_SUBAGENT_MODEL: ["minimax-m3", "qwen3.7-plus", "deepseek-v4-flash", "kimi-k3"],
  ANTHROPIC_CUSTOM_MODEL_OPTION: ["kimi-k3", "glm-5.2", "qwen3.7-max", "deepseek-v4-pro"],
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

function powerShellLiteral(value: string): string {
  return `'${value.replaceAll("'", "''")}'`;
}

function posixShellLiteral(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

// Effective OpenCode Go limits and capabilities, verified 2026-08-14; not generic vendor defaults.
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
  "gpt-5.6-luna": {
    // models.dev opencode-go/gpt-5.6-luna: context 1_050_000, output 128_000; image+pdf input.
    contextWindow: 1_050_000,
    maxOutputTokens: 128_000,
    input: ["text", "image"],
    reasoning: true,
    toolUse: true,
    efforts: ["low", "medium", "high", "max"],
    defaultEffort: "medium",
  },
  "muse-spark-1.2": {
    // Meta Muse Spark 1.2 via OpenCode Go: context 1_048_576, output 131_072;
    // text+image input; Responses-only. Standard price comes from live Go
    // measurement because the public Go pricing table lists only Contributor.
    // Upstream accepts {none,minimal,low,medium,high,xhigh} but NOT `max`;
    // expose low/medium/high only so clients never send the unsupported `max`.
    contextWindow: 1_048_576,
    maxOutputTokens: 131_072,
    input: ["text", "image"],
    reasoning: true,
    toolUse: true,
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
  },
  "muse-spark-1.2-contributor": {
    // Same checkpoint as muse-spark-1.2. Contributor is not ZDR: prompts and
    // completions may be used for training; the Applications view warns on selection.
    contextWindow: 1_048_576,
    maxOutputTokens: 131_072,
    input: ["text", "image"],
    reasoning: true,
    toolUse: true,
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
  },
  "glm-5.3": {
    // models.dev opencode-go/glm-5.3 (2026-08-17): context 1_000_000, output 131_072;
    // cost $1.40 / $4.40 / $0.26 matches the official Go table; efforts low/high/max.
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    efforts: ["low", "high", "max"],
    defaultEffort: "max",
    piThinkingLevelMap: {
      off: null,
      minimal: null,
      low: "low",
      medium: null,
      high: "high",
      xhigh: null,
      max: "max",
    },
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
  "qwen3.8-max": {
    // models.dev opencode-go/qwen3.8-max: context 1_000_000, output 131_072.
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    piThinkingLevelMap: PI_MINIMAL_TO_LOW,
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
  hy3: {
    // models.dev opencode-go/hy3: context 256_000, output 64_000; text-only.
    contextWindow: 256_000,
    maxOutputTokens: 64_000,
    input: ["text"],
    reasoning: true,
    toolUse: true,
    efforts: ["low", "high"],
    defaultEffort: "high",
  },
};

function applicationModelMetadata(modelId: string): ApplicationModelMetadata {
  const metadata = APPLICATION_MODEL_METADATA[modelId];
  if (!metadata) {
    throw new Error(`Missing verified application model metadata for ${JSON.stringify(modelId)}`);
  }
  return metadata;
}

function codexCatalogModelIds(context: GuideContext): string[] {
  const fallback = context.modelValues.model || context.modelId;
  const source = context.availableModelIds.length
    ? context.availableModelIds
    : fallback && fallback !== "<MODEL_ID>"
      ? [fallback]
      : [];
  return [...new Set(source.filter((modelId) => modelId && modelId !== "<MODEL_ID>"))];
}

// Current Codex rejects a catalog that omits these fields. Keep the text
// short: enabling the catalog replaces Codex's bundled models and this
// template, instead of the official 20KB agent prompt used by slug fallback.
const CODEX_CATALOG_BASE_INSTRUCTIONS =
  "You are a coding agent. Inspect the workspace, use tools to run commands and apply patches, and prefer making the change over only describing it. Follow repository conventions and keep replies concise.";

export function buildCodexModelCatalog(context: GuideContext) {
  return {
    models: codexCatalogModelIds(context).map((modelId, index) => {
      const metadata = applicationModelMetadata(modelId);
      const effectiveInput = metadata.ocgInput ?? metadata.input;
      const entry: Record<string, unknown> = {
        slug: modelId,
        display_name: modelId,
        description: `${modelId} via Open Console Gateway`,
        base_instructions: CODEX_CATALOG_BASE_INSTRUCTIONS,
        context_window: metadata.contextWindow,
        max_context_window: metadata.contextWindow,
        effective_context_window_percent: 95,
        input_modalities: effectiveInput.includes("image") ? ["text", "image"] : ["text"],
        supported_in_api: true,
        // Codex requires this field for every catalog model, including
        // models that expose no selectable reasoning effort.
        supported_reasoning_levels: (metadata.efforts ?? []).map((effort) => ({
          effort,
          description: effort,
        })),
        visibility: "list",
        shell_type: "default",
        priority: 10 + index,
        support_verbosity: false,
        truncation_policy: { mode: "bytes", limit: 10_000 },
        experimental_supported_tools: [],
      };
      if (metadata.defaultEffort) {
        entry.default_reasoning_level = metadata.defaultEffort;
      }
      return entry;
    }),
  };
}

function buildCodexProviderConfig(context: GuideContext): string {
  const model = context.modelValues.model || context.modelId;
  const reviewModel = context.modelValues.review_model || context.modelId;
  return [
    `model = ${JSON.stringify(model)}`,
    `review_model = ${JSON.stringify(reviewModel)}`,
    `model_provider = "ocg"`,
    "# Optional. Uncomment only after saving ocg-model-catalog.json.",
    "# A catalog replaces Codex's bundled model list for this process.",
    '# model_catalog_json = "ocg-model-catalog.json"',
    "",
    "[model_providers.ocg]",
    'name = "Open Console Gateway"',
    `base_url = ${JSON.stringify(context.apiBaseUrl)}`,
    'env_key = "OCG_API_KEY"',
    'wire_api = "responses"',
    "requires_openai_auth = false",
  ].join("\n");
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
  return `[models.${JSON.stringify(alias)}]\nprovider = "ocg"\nmodel = ${JSON.stringify(modelId)}\nmax_context_size = ${metadata.contextWindow}\ncapabilities = ${JSON.stringify(kimiCodeCapabilities(metadata))}\ndisplay_name = ${JSON.stringify(`${modelId} (Open Console Gateway)`)}${effortLines}`;
}

function kimiTemporaryLaunch(context: GuideContext, key: string, shell: "powershell" | "bash"): string {
  const metadata = applicationModelMetadata(context.modelId);
  const values = {
    KIMI_MODEL_NAME: context.modelId,
    KIMI_MODEL_API_KEY: key,
    KIMI_MODEL_PROVIDER_TYPE: "openai",
    KIMI_MODEL_BASE_URL: context.apiBaseUrl,
    KIMI_MODEL_MAX_CONTEXT_SIZE: String(metadata.contextWindow),
    KIMI_MODEL_CAPABILITIES: kimiCodeCapabilities(metadata).join(","),
  };
  if (shell === "powershell") {
    return `${Object.entries(values)
      .map(([name, value]) => `$env:${name} = ${powerShellLiteral(value)}`)
      .join("\n")}\nkimi`;
  }
  return `${Object.entries(values)
    .map(([name, value]) => `export ${name}=${posixShellLiteral(value)}`)
    .join("\n")}\nkimi`;
}

function openClawOnboardingCommand(
  context: GuideContext,
  key: string,
  shell: "powershell" | "bash",
): string {
  const metadata = applicationModelMetadata(context.modelId);
  const inputFlag = (metadata.ocgInput ?? metadata.input).includes("image")
    ? "--custom-image-input"
    : "--custom-text-input";
  if (shell === "powershell") {
    return `$env:CUSTOM_API_KEY = ${powerShellLiteral(key)}\nopenclaw onboard --non-interactive --accept-risk \`\n  --mode local \`\n  --auth-choice custom-api-key \`\n  --custom-base-url ${powerShellLiteral(context.apiBaseUrl)} \`\n  --custom-model-id ${powerShellLiteral(context.modelId)} \`\n  --secret-input-mode ref \`\n  --custom-provider-id ocg \`\n  --custom-compatibility openai \`\n  ${inputFlag} \`\n  --gateway-bind loopback`;
  }
  return `export CUSTOM_API_KEY=${posixShellLiteral(key)}\nopenclaw onboard --non-interactive --accept-risk \\\n  --mode local \\\n  --auth-choice custom-api-key \\\n  --custom-base-url ${posixShellLiteral(context.apiBaseUrl)} \\\n  --custom-model-id ${posixShellLiteral(context.modelId)} \\\n  --secret-input-mode ref \\\n  --custom-provider-id ocg \\\n  --custom-compatibility openai \\\n  ${inputFlag} \\\n  --gateway-bind loopback`;
}

function workBuddyForm(context: GuideContext, key: string): string {
  const metadata = applicationModelMetadata(context.modelId);
  const effectiveInput = metadata.ocgInput ?? metadata.input;
  const state = (enabled: boolean) => enabled ? "On" : "Off";
  return `Provider: Custom\nURL: ${context.chatCompletionsUrl}\nAPI Key: ${key}\nModel: ${context.modelId}\nCustom Protocol: Off\nTool Calling: ${state(Boolean(metadata.toolUse))}\nImage Input: ${state(effectiveInput.includes("image"))}\nReasoning Mode: ${state(Boolean(metadata.reasoning))}`;
}

function openClawModelConfig(modelId: string) {
  const metadata = applicationModelMetadata(modelId);
  const effectiveInput = metadata.ocgInput ?? metadata.input;
  return {
    id: modelId,
    name: modelId,
    reasoning: Boolean(metadata.reasoning),
    input: ["text", ...(effectiveInput.includes("image") ? ["image"] : [])],
    contextWindow: metadata.contextWindow,
    maxTokens: metadata.maxOutputTokens,
  };
}

function hermesModelEntry(modelId: string): string {
  const metadata = applicationModelMetadata(modelId);
  const effectiveInput = metadata.ocgInput ?? metadata.input;
  return `      ${JSON.stringify(modelId)}:\n        context_length: ${metadata.contextWindow}\n        supports_vision: ${effectiveInput.includes("image")}`;
}

function vscodeTokenLimits(modelId: string) {
  const metadata = APPLICATION_MODEL_METADATA[modelId];
  // Unknown future models keep conservative limits until their real window is added above.
  if (!metadata) return { maxInputTokens: 32_768, maxOutputTokens: 8_192 };
  return {
    maxInputTokens: metadata.contextWindow - metadata.maxOutputTokens,
    maxOutputTokens: metadata.maxOutputTokens,
  };
}

export const APPLICATION_GUIDES = [
  {
    id: "claude-code",
    name: "Claude Code",
    category: "Claude 兼容",
    protocol: "Anthropic Messages",
    endpointKind: "messages",
    officialUrl: "https://code.claude.com/docs/en/llm-gateway-connect",
    summary: "通过 Anthropic 兼容入口连接 Open Console Gateway，地址使用不带 /v1 的根地址。",
    steps: [
      "打开用户级 ~/.claude/settings.json，将下面的环境变量和模型配置合并进去。",
      "确认 ANTHROPIC_BASE_URL 使用下方根地址，ANTHROPIC_AUTH_TOKEN 使用 Key。",
      "启动 Claude Code 并发送一条测试消息，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "Claude Code 使用 Anthropic Messages 协议，因此不要给 ANTHROPIC_BASE_URL 追加 /v1。",
      "Claude Code 走 Messages 协议；默认推荐支持 Messages 透传的模型。Chat-only 模型会由 Gateway 转换。",
      "团队管理员可通过 Managed Settings、设备管理或 apiKeyHelper 下发 Claude Code；个人用户合并下方配置。",
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
    officialUrl: "https://claude.com/docs/third-party/claude-desktop/gateway",
    summary: "通过 Anthropic 兼容入口连接 Open Console Gateway，地址使用不带 /v1 的根地址。",
    steps: [
      "先在 Help → Troubleshooting → Enable Developer Mode 打开开发者模式，然后重启 Claude Desktop。",
      "打开 Claude Desktop 的 Developer → Configure Third-Party Inference，选择 Gateway。",
      "填写下方 Gateway base URL 和 Key；三个角色模型在本页选择，桌面窗口不填模型 ID。",
      "发送一条测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "Claude Desktop 可从配置窗口导出 .reg 或 .mobileconfig 供团队部署；个人用户直接使用该窗口。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    modelFields: ["sonnet", "opus", "haiku"],
    snippets: (context) => [
      keyedSnippet(
        context,
        "Developer → Configure Third-Party Inference",
        "text",
        (key) => `Inference provider: Gateway\nGateway base URL: ${context.rootUrl}/claude-desktop\nCredential kind: Static API key\nGateway API key: ${key}\nGateway auth scheme: Bearer`,
      ),
    ],
  },
  {
    id: "codex",
    name: "Codex",
    category: "OpenAI 兼容",
    protocol: "OpenAI Responses",
    endpointKind: "responses",
    officialUrl: "https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers",
    badge: "Responses",
    summary: "注册 Open Console Gateway 为 Codex 自定义模型提供商，通过 Responses 接口调用。",
    steps: [
      "CLI 切换：保存 ~/.codex/ocg.config.toml 后运行 codex --profile ocg；Desktop 或默认提供商：把相同配置合并进用户级 ~/.codex/config.toml。",
      "可选：把模型目录保存为 ~/.codex/ocg-model-catalog.json，并在 toml 里取消注释 model_catalog_json。",
      "在启动 Codex 的同一终端设置 OCG_API_KEY 环境变量。",
      "启动 Codex 并发送一条测试消息，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "Codex 自定义 provider 必须 wire_api = \"responses\"；当前 Codex 不再支持 chat wire_api。",
      "Open Console Gateway 原生接收 /v1/responses；若上游更偏好其他协议，Gateway 会转换，无需再叠一层 Chat 转换器。",
      "model_catalog_json 可选。不写也能请求，未知模型按 272K 回退。写了会整份替换 Codex 内置目录，用来提供选择器、真实上下文窗口和推理档位。",
      "Open Console Gateway 当前提供无状态 Responses 转发，不要依赖 previous_response_id 延续服务端状态。",
      "项目内 .codex/config.toml 不能配置 model_providers；provider 必须写在用户级配置或 profile 文件。",
      "Desktop 更适合合并用户级 config.toml；CLI 可用 profile 避免改默认配置。合并 config.toml 会切换默认 model_provider。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    modelFields: ["model", "review_model"],
    snippets: (context) => {
      const providerConfig = buildCodexProviderConfig(context);
      const catalog = JSON.stringify(buildCodexModelCatalog(context), null, 2);
      return [
        {
          label: "~/.codex/ocg-model-catalog.json",
          language: "json",
          display: catalog,
          copy: catalog,
        },
        {
          label: "~/.codex/ocg.config.toml",
          language: "toml",
          display: providerConfig,
          copy: providerConfig,
        },
        {
          label: "~/.codex/config.toml",
          language: "toml",
          display: `# Merge into user-level ~/.codex/config.toml (changes default model_provider)\n${providerConfig}`,
          copy: `# Merge into user-level ~/.codex/config.toml (changes default model_provider)\n${providerConfig}`,
        },
        keyedSnippet(
          context,
          t("当前 PowerShell 会话"),
          "powershell",
          (key) => `$env:OCG_API_KEY = ${powerShellLiteral(key)}\n# Profile (recommended for CLI):\ncodex --profile ocg\n# After merging ~/.codex/config.toml:\n# codex`,
        ),
        keyedSnippet(
          context,
          "macOS / Linux shell",
          "bash",
          (key) => `export OCG_API_KEY=${posixShellLiteral(key)}\n# Profile (recommended for CLI):\ncodex --profile ocg\n# After merging ~/.codex/config.toml:\n# codex`,
        ),
      ];
    },
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
      "发送一条测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
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
    badge: "原生插件",
    summary: "把 Open Console Gateway 作为原生 Provider 扩展安装到 Pi，不直接改写 models.json。",
    steps: [
      "在上方选择要暴露给 Pi 的 Alias，然后使用本机接入面板安装插件。",
      "重新启动 Pi，在 Provider 的原生登录入口为 Open Console Gateway 保存当前 Key。",
      "选择 ocg-manager 下的模型发送测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "插件由 Pi 自己的 package manager 安装与卸载；Open Console Gateway 不接管 Pi 的其他扩展。",
      "Key 通过 Pi 的原生 auth/login 流程保存，不会写入插件源码或生成的模型目录。",
      "重新安装只更新 OCG 自有 Provider 与模型清单，不覆盖内置 Provider。",
    ],
    snippets: (context) => [
      {
        label: "Pi plugin catalog preview",
        language: "json",
        display: JSON.stringify({
          providers: {
            ocg: {
              baseUrl: context.apiBaseUrl,
              api: "openai-completions",
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
              compat: PI_PROVIDER_COMPAT,
              models: models(context).filter(Boolean).map(piModelConfig),
            },
          },
        }, null, 2),
      },
      keyedSnippet(
        context,
        "Pi Provider login",
        "text",
        (key) => `Provider: ocg-manager\nKey: ${key}`,
      ),
      {
        label: "Pi provider",
        language: "text",
        display: `Provider: ocg-manager\nBase URL: ${context.apiBaseUrl}\nModels: ${models(context).join(", ")}`,
        copy: `Provider: ocg-manager\nBase URL: ${context.apiBaseUrl}\nModels: ${models(context).join(", ")}`,
      },
    ],
    multipleModels: true,
  },
  {
    id: "dsh",
    name: "DeepSeek Harness",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://github.com/deepseek-ai/deepseek-harness",
    badge: "原生插件",
    summary: "把 Open Console Gateway 作为原生 Bundle 安装到 DSH 的 web profile，不覆盖现有 Harness 配置。",
    steps: [
      "在上方选择要暴露给 DSH 的 Alias，然后使用本机接入面板安装插件。",
      "重新启动 DSH web profile，在 Models 页面为 OCG_MANAGER_API_KEY 保存当前 Key。",
      "选择 ocg-manager 下的模型发送测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "第一阶段只管理 DSH 的 web profile；headless、TUI 和自定义 profile 暂不自动安装。",
      "插件复用 DSH 自带的 llm-pi-ai adapter、Settings 与 Credentials，不创建第二套 Agent 或模型运行时。",
      "卸载只移除 ocg-manager-dsh Bundle，不改动其他 DSH 插件或 Provider。",
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        "DSH Models → Credentials",
        "dotenv",
        (key) => `OCG_MANAGER_API_KEY=${JSON.stringify(key)}`,
      ),
      {
        label: "DSH provider",
        language: "text",
        display: `Profile: web\nProvider: ocg-manager\nBase URL: ${context.apiBaseUrl}\nModels: ${models(context).join(", ")}`,
        copy: `Profile: web\nProvider: ocg-manager\nBase URL: ${context.apiBaseUrl}\nModels: ${models(context).join(", ")}`,
      },
    ],
    multipleModels: true,
  },
  {
    id: "kimi-code",
    name: "Kimi Code CLI",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/env-vars.html",
    summary: "优先用 KIMI_MODEL_* 临时启动，不修改 config.toml；需要多模型或长期使用时再写入持久配置。",
    steps: [
      "复制并运行当前平台的临时启动命令；Kimi Code CLI 会在内存中创建 OCG Provider。",
      "把下面的 provider 与 model 配置合并到用户级 ~/.kimi-code/config.toml。",
      "启动 Kimi Code CLI 并发送一条测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "Kimi CLI 已迁移到 Kimi Code CLI；新接入使用 ~/.kimi-code 而不是旧版 ~/.kimi。",
      "Kimi Code CLI 会把 api_key 明文保存在 config.toml；请限制配置目录权限。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        "临时启动（PowerShell）",
        "powershell",
        (key) => kimiTemporaryLaunch(context, key, "powershell"),
      ),
      keyedSnippet(
        context,
        "临时启动（macOS / Linux）",
        "bash",
        (key) => kimiTemporaryLaunch(context, key, "bash"),
      ),
      keyedSnippet(context, "~/.kimi-code/config.toml（持久配置）", "toml", (key) => {
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
    summary: "使用 OpenAI Compatible AI SDK provider，将 Open Console Gateway 注册为自定义服务商。",
    steps: [
      "把下面的 provider 配置保存为 ~/.config/opencode/ocg.json。",
      "设置 OCG_API_KEY 和 OPENCODE_CONFIG 后启动 OpenCode。",
      "在 OpenCode 中发送一条测试消息，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "用 OPENCODE_CONFIG 指向独立配置文件，可以避免修改默认 OpenCode 配置。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(context, "~/.config/opencode/ocg.json", "json", () =>
        JSON.stringify(
          {
            $schema: "https://opencode.ai/config.json",
            provider: {
              ocg: {
                npm: "@ai-sdk/openai-compatible",
                name: "Open Console Gateway",
                options: { baseURL: context.apiBaseUrl, apiKey: "{env:OCG_API_KEY}" },
                models: Object.fromEntries(models(context).map((modelId) => [
                  modelId,
                  { name: modelId, reasoning: applicationModelMetadata(modelId).reasoning },
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
        (key) => `$env:OCG_API_KEY = ${powerShellLiteral(key)}\n$env:OPENCODE_CONFIG = Join-Path $HOME '.config/opencode/ocg.json'\nopencode`,
      ),
      keyedSnippet(
        context,
        "macOS / Linux shell",
        "bash",
        (key) => `export OCG_API_KEY=${posixShellLiteral(key)}\nexport OPENCODE_CONFIG="$HOME/.config/opencode/ocg.json"\nopencode`,
      ),
    ],
    multipleModels: true,
  },
  {
    id: "workbuddy",
    name: "WorkBuddy",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    officialUrl: "https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model",
    badge: "GUI",
    summary: "通过 WorkBuddy 官方可视化自定义模型表单接入 Open Console Gateway，无需编辑 JSON。",
    steps: [
      "打开 WorkBuddy 设置 → 模型 → 自定义模型，点击添加模型并选择 自定义/Custom。",
      "按下方参数填写完整 Chat Completions 地址、Key、模型 ID 与能力开关。",
      "保持自定义协议关闭，保存后从对话模型选择器切换到该模型并发送测试任务。",
    ],
    notes: [
      "WorkBuddy 没有公开模型配置导入协议；OCG 当前应使用官方可视化表单。",
      "接口地址使用完整 /v1/chat/completions；该路径是标准 OpenAI Chat Completions，因此自定义协议保持关闭。",
      "每个模型需要单独保存；工具调用、图片输入和推理开关必须与模型能力一致。",
      "API Key 仅保存在本机模型配置中，但共享设备仍不应保存或导出。",
    ],
    snippets: (context) => [
      keyedSnippet(context, "设置 → 模型 → 自定义模型", "text", (key) => workBuddyForm(context, key)),
    ],
  },
  {
    id: "openclaw",
    name: "OpenClaw",
    category: "OpenAI 兼容",
    protocol: "OpenAI Chat Completions",
    endpointKind: "chat",
    popular: true,
    officialUrl: "https://docs.openclaw.ai/start/wizard-cli-automation",
    summary: "首次安装优先运行官方非交互 onboarding；已有配置或需要多模型时使用下方 JSON 和 .env。",
    steps: [
      "首次配置时运行下方 onboarding 命令；命令会修改 OpenClaw 本机配置，请先确认内容。",
      "填写下方 Base URL、Key 和模型 ID。",
      "运行 openclaw models status --probe --probe-provider ocg，再到 Open Console Gateway 请求日志确认真实调用。",
    ],
    notes: [
      "onboarding 的 ref 模式固定引用 CUSTOM_API_KEY；下方 .env 与手工 JSON 使用同一变量。",
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "已有配置或需要多模型时使用下方 JSON；模型上下文、输出上限与图片能力按已验证元数据生成。",
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
                apiKey: "${CUSTOM_API_KEY}",
                api: "openai-completions",
                models: models(context).map(openClawModelConfig),
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
        keyedSnippet(
          context,
          "首次配置（PowerShell）",
          "powershell",
          (key) => openClawOnboardingCommand(context, key, "powershell"),
        ),
        keyedSnippet(
          context,
          "首次配置（macOS / Linux）",
          "bash",
          (key) => openClawOnboardingCommand(context, key, "bash"),
        ),
        { label: "~/.openclaw/openclaw.json", language: "json5", display: config, copy: config },
        keyedSnippet(
          context,
          "~/.openclaw/.env",
          "dotenv",
          (key) => `CUSTOM_API_KEY=${JSON.stringify(key)}`,
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
    popular: true,
    officialUrl: "https://hermes-agent.nousresearch.com/docs/integrations/providers",
    summary: "运行 hermes model 的 Custom endpoint 向导完成首次接入；长期或多模型使用 key_env 配置。",
    steps: [
      "运行 hermes model，选择 Custom endpoint，再填写下方地址、Key 和模型。",
      "transport 使用 chat_completions，并按下方元数据填写 context_length 与图片能力。",
      "运行 hermes chat -q 发送测试任务，再到 Open Console Gateway 请求日志确认真实调用。",
    ],
    notes: [
      "baseURL 必须使用带 /v1 的 API Base URL。",
      "hermes model 向导可能把单次接入写到 model.base_url；长期或多模型请用下方 providers.ocg 与 key_env，把 Key 放在 ~/.hermes/.env。",
      "下方配置按模型元数据固定 context_length 和 supports_vision，避免自动探测不完整。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      {
        label: "~/.hermes/config.yaml",
        language: "yaml",
        display: `providers:\n  ocg:\n    api: ${JSON.stringify(context.apiBaseUrl)}\n    key_env: OCG_API_KEY\n    transport: chat_completions\n    models:\n${models(context).map(hermesModelEntry).join("\n")}\n\nmodel:\n  default: ${JSON.stringify(context.modelId)}\n  provider: custom:ocg`,
        copy: `providers:\n  ocg:\n    api: ${JSON.stringify(context.apiBaseUrl)}\n    key_env: OCG_API_KEY\n    transport: chat_completions\n    models:\n${models(context).map(hermesModelEntry).join("\n")}\n\nmodel:\n  default: ${JSON.stringify(context.modelId)}\n  provider: custom:ocg`,
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
    officialUrl: "https://docs.cherry-ai.com/en-us/pre-basic/providers/zi-ding-yi-fu-wu-shang",
    summary: "在服务商设置中新增 OpenAI 类型的自定义服务商，并获取模型列表。",
    steps: [
      "进入设置 → 模型服务，新增 OpenAI 类型的自定义服务商。",
      "填写 API 地址和 Key 后，点击获取模型列表并勾选需要的模型。",
      "执行连接检查或发送一条测试消息，再到 Open Console Gateway 的请求日志确认成功记录。",
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
      "在 Chat 中选择该模型并发送测试消息，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "BYOK 只影响支持自带密钥的聊天模型，不接管 Copilot 行内补全、embedding 等能力。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      keyedSnippet(context, "chatLanguageModels.json", "json", (key) =>
        JSON.stringify(
          [{
            name: "Open Console Gateway",
            vendor: "customendpoint",
            apiKey: key,
            apiType: "chat-completions",
            models: models(context).map((modelId) => ({
              id: modelId,
              name: modelId,
              url: context.chatCompletionsUrl,
              toolCalling: true,
              vision: (applicationModelMetadata(modelId).ocgInput
                ?? applicationModelMetadata(modelId).input).includes("image"),
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
    summary: "选择 OpenAI Compatible provider，直接填写 Open Console Gateway 的 API Base URL。",
    steps: [
      "打开 Cline 设置，将 API Provider 选择为 OpenAI Compatible。",
      "填写下方 Base URL、Key 和模型 ID。",
      "发送一条测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
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
    officialUrl: "https://roocodeinc.github.io/Roo-Code/features/settings-management/",
    summary: "选择 OpenAI Compatible provider，将对话请求转发到 Open Console Gateway。",
    steps: [
      "打开 Roo Code 配置，将 API Provider 选择为 OpenAI Compatible。",
      "填写下方 Base URL、Key 和模型 ID。",
      "首次配置后导出 roo-code-settings.json；其他环境可用 Import 或 roo-cline.autoImportSettingsPath 自动导入。",
      "发送一条测试任务，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "Roo Code 导出文件包含明文 API Key，只能保存在可信位置。",
      "Roo Code 仅支持原生工具调用；所选模型不支持 tools 时无法使用 Agent 模式。",
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        t("Provider 参数"),
        "text",
        (key) => `Base URL: ${context.apiBaseUrl}\nAPI Key: ${key}\nModel ID: ${context.modelId}`,
      ),
      {
        label: "VS Code settings.json（复用已导出配置）",
        language: "json",
        display: JSON.stringify({ "roo-cline.autoImportSettingsPath": "~/roo-code-settings.json" }, null, 2),
        copy: JSON.stringify({ "roo-cline.autoImportSettingsPath": "~/roo-code-settings.json" }, null, 2),
      },
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
      "把 Key 写入 ~/.continue/.env，YAML 通过 secrets.OCG_API_KEY 引用。",
      "保持 provider 为 openai、apiBase 使用 /v1 地址、useResponsesApi 为 false。",
      "选择 Open Console Gateway 模型发送测试消息，再到请求日志确认成功记录。",
    ],
    notes: [
      "useResponsesApi: false 用于明确走 Chat Completions 兼容路径。",
      "Continue Hub 适合共享固定配置；每个 OCG 节点的地址和 Key 不同时，本机 YAML 更直接。",
      "模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。",
    ],
    snippets: (context) => [
      {
        label: "Continue YAML",
        language: "yaml",
        display: `name: Open Console Gateway\nversion: 1.0.0\nschema: v1\nmodels:\n${models(context).map((modelId) => `  - name: ${JSON.stringify(`${modelId} (OCG)`)}\n    provider: openai\n    model: ${JSON.stringify(modelId)}\n    apiBase: ${JSON.stringify(context.apiBaseUrl)}\n    apiKey: \${{ secrets.OCG_API_KEY }}\n    useResponsesApi: false\n    capabilities:\n      - tool_use`).join("\n")}`,
        copy: `name: Open Console Gateway\nversion: 1.0.0\nschema: v1\nmodels:\n${models(context).map((modelId) => `  - name: ${JSON.stringify(`${modelId} (OCG)`)}\n    provider: openai\n    model: ${JSON.stringify(modelId)}\n    apiBase: ${JSON.stringify(context.apiBaseUrl)}\n    apiKey: \${{ secrets.OCG_API_KEY }}\n    useResponsesApi: false\n    capabilities:\n      - tool_use`).join("\n")}`,
      },
      keyedSnippet(context, "~/.continue/.env", "dotenv", (key) => `OCG_API_KEY=${JSON.stringify(key)}`),
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
    summary: "新增 OpenAI API 类型提供商，API Host 使用 Open Console Gateway 根地址。",
    steps: [
      "优先点击上方一键导入；若客户端未安装或浏览器阻止协议链接，再按下方参数手动添加。",
      "填写下方 API Host、Key 和模型 ID，保留默认的 /v1/chat/completions 路径。",
      "发送一条测试消息，再到 Open Console Gateway 的请求日志确认成功记录。",
    ],
    notes: [
      "Chatbox 深链中的 Base64 只是编码，不是加密；不要分享包含 Key 的导入链接。",
      "API Host 使用不带 /v1 的根地址，避免形成重复路径。",
    ],
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
