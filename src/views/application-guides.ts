import { t } from '../i18n/index.ts';
import type { MessageKey } from '../i18n/index.ts';

const APPLICATION_IDS = [
  'claude-code',
  'codex',
  'opencode',
  'cherry-studio',
  'vscode-copilot',
  'trae',
  'cline',
  'roo-code',
  'continue',
  'chatbox',
] as const;

export type ApplicationId = (typeof APPLICATION_IDS)[number];

export interface GuideContext {
  rootUrl: string;
  apiBaseUrl: string;
  chatCompletionsUrl: string;
  responsesUrl: string;
  messagesUrl: string;
  displayKey: string;
  actualKey: string;
}

export interface GuideSnippet {
  label: string;
  language: string;
  display: string;
  copy: string;
}

export interface ApplicationGuide {
  id: ApplicationId;
  name: string;
  protocol: string;
  officialUrl: string;
  badge?: string;
  summary: MessageKey;
  steps: readonly MessageKey[];
  notes: readonly MessageKey[];
  snippets: (context: GuideContext) => GuideSnippet[];
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

export const APPLICATION_GUIDES: readonly ApplicationGuide[] = [
  {
    id: 'claude-code',
    name: 'Claude Code',
    protocol: 'Anthropic Messages',
    officialUrl: 'https://code.claude.com/docs/en/llm-gateway-connect',
    summary: '通过 Anthropic 兼容入口连接 OCG Manager，地址使用不带 /v1 的根地址。',
    steps: [
      '打开用户级 ~/.claude/settings.json，将下面的环境变量和模型配置合并进去。',
      '确认 ANTHROPIC_BASE_URL 使用下方根地址，ANTHROPIC_AUTH_TOKEN 使用 Gateway Key。',
      '启动 Claude Code 并发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: [
      '示例模型为 minimax-m3；如节点实际可用模型不同，请同步修改 model。',
      'Claude Code 使用 Anthropic Messages 协议，因此不要给 ANTHROPIC_BASE_URL 追加 /v1。',
    ],
    snippets: (context) => [
      keyedSnippet(context, '~/.claude/settings.json', 'json', (key) =>
        JSON.stringify(
          {
            env: {
              ANTHROPIC_BASE_URL: context.rootUrl,
              ANTHROPIC_AUTH_TOKEN: key,
            },
            model: 'minimax-m3',
          },
          null,
          2,
        ),
      ),
    ],
  },
  {
    id: 'codex',
    name: 'Codex',
    protocol: 'OpenAI Responses',
    officialUrl: 'https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers',
    badge: 'Responses',
    summary: '注册 OCG Manager 为 Codex 自定义模型提供商，通过 Responses 接口调用。',
    steps: [
      '把模型与 provider 配置写入用户级 ~/.codex/config.toml。',
      '在启动 Codex 的同一终端设置 OCG_API_KEY 环境变量。',
      '启动 Codex 并发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: [
      'OCG Manager 当前提供无状态 Responses 转发，不要依赖 previous_response_id 延续服务端状态。',
      '示例模型为 glm-5.2；如节点实际可用模型不同，请同步修改 model。',
    ],
    snippets: (context) => [
      {
        label: '~/.codex/config.toml',
        language: 'toml',
        display: `model = "glm-5.2"\nmodel_provider = "ocg"\n\n[model_providers.ocg]\nname = "OCG Manager"\nbase_url = "${context.apiBaseUrl}"\nenv_key = "OCG_API_KEY"\nwire_api = "responses"`,
        copy: `model = "glm-5.2"\nmodel_provider = "ocg"\n\n[model_providers.ocg]\nname = "OCG Manager"\nbase_url = "${context.apiBaseUrl}"\nenv_key = "OCG_API_KEY"\nwire_api = "responses"`,
      },
      keyedSnippet(
        context,
        t('当前 PowerShell 会话'),
        'powershell',
        (key) => `$env:OCG_API_KEY = ${JSON.stringify(key)}`,
      ),
    ],
  },
  {
    id: 'opencode',
    name: 'OpenCode',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://opencode.ai/docs/providers/',
    summary: '使用 OpenAI Compatible AI SDK provider，将 OCG Manager 注册为自定义服务商。',
    steps: [
      '把下面的 provider 配置合并到项目或用户级 opencode.json。',
      '按节点可用模型调整 models 和默认 model，保留 npm 为 @ai-sdk/openai-compatible。',
      '在 OpenCode 中发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: ['baseURL 必须使用带 /v1 的 API Base URL。'],
    snippets: (context) => [
      keyedSnippet(context, 'opencode.json', 'json', (key) =>
        JSON.stringify(
          {
            $schema: 'https://opencode.ai/config.json',
            provider: {
              ocg: {
                npm: '@ai-sdk/openai-compatible',
                name: 'OCG Manager',
                options: { baseURL: context.apiBaseUrl, apiKey: key },
                models: { 'minimax-m3': { name: 'MiniMax M3' } },
              },
            },
            model: 'ocg/minimax-m3',
          },
          null,
          2,
        ),
      ),
    ],
  },
  {
    id: 'cherry-studio',
    name: 'Cherry Studio',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://docs.cherry-ai.com/docs/en-us/pre-basic/settings/providers',
    summary: '在服务商设置中新增 OpenAI 类型的自定义服务商，并手工添加可用模型。',
    steps: [
      '进入设置 → 模型服务，新增 OpenAI 类型的自定义服务商。',
      '按下方参数填写 API 地址和 Key，并手工添加示例模型 minimax-m3。',
      '执行连接检查或发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: ['API 地址使用不带 /v1 的根地址，由 Cherry Studio 补全 OpenAI 请求路径。'],
    snippets: (context) => [
      keyedSnippet(
        context,
        t('服务商参数'),
        'text',
        (key) =>
          t('服务商类型: OpenAI\nAPI 地址: {url}\nAPI Key: {key}\n模型 ID: {model}', {
            url: context.rootUrl,
            key,
            model: 'minimax-m3',
          }),
      ),
    ],
  },
  {
    id: 'vscode-copilot',
    name: 'VS Code Copilot Chat',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://code.visualstudio.com/docs/agent-customization/language-models',
    badge: 'BYOK',
    summary: '在 Copilot Chat 的自带密钥模型设置中添加 OpenAI Compatible 完整端点。',
    steps: [
      '在 Copilot Chat 的模型管理中选择添加 OpenAI Compatible 模型。',
      '填写下方完整 Chat Completions Endpoint、Key 和模型 ID。',
      '在 Chat 中选择该模型并发送测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: [
      'BYOK 只影响支持自带密钥的聊天模型，不接管 Copilot 行内补全、embedding 等能力。',
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        t('Custom Endpoint 参数'),
        'text',
        (key) =>
          `Endpoint: ${context.chatCompletionsUrl}\nAPI Key: ${key}\nModel ID: minimax-m3`,
      ),
    ],
  },
  {
    id: 'trae',
    name: 'Trae',
    protocol: 'OpenAI Compatible',
    officialUrl: 'https://docs.trae.ai/ide/models',
    badge: '版本相关',
    summary: '在支持自定义模型的 Trae 版本中填写 OCG Manager 的 OpenAI Compatible 参数。',
    steps: [
      '打开模型管理，确认当前 Trae 版本提供自定义 OpenAI Compatible 模型入口。',
      '按下方参数填写 Base URL、Key 和模型 ID。',
      '选择该模型发送测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: [
      'Trae 的自定义模型入口和字段可能随版本、渠道或地区变化，请以当前客户端界面为准。',
      '此页不宣称所有 Trae 版本均兼容。',
    ],
    snippets: (context) => [
      keyedSnippet(
        context,
        t('自定义模型参数'),
        'text',
        (key) => `Base URL: ${context.apiBaseUrl}\nAPI Key: ${key}\nModel ID: minimax-m3`,
      ),
    ],
  },
  {
    id: 'cline',
    name: 'Cline',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://docs.cline.bot/provider-config/openai-compatible',
    summary: '选择 OpenAI Compatible provider，直接填写 OCG Manager 的 API Base URL。',
    steps: [
      '打开 Cline 设置，将 API Provider 选择为 OpenAI Compatible。',
      '填写下方 Base URL、Key 和模型 ID。',
      '发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: ['模型能力由实际上游决定；Agent 工具调用需要所选模型正确支持 tools。'],
    snippets: (context) => [
      keyedSnippet(
        context,
        t('Provider 参数'),
        'text',
        (key) => `Base URL: ${context.apiBaseUrl}\nAPI Key: ${key}\nModel ID: minimax-m3`,
      ),
    ],
  },
  {
    id: 'roo-code',
    name: 'Roo Code',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://docs.roocode.com/providers/openai-compatible',
    summary: '选择 OpenAI Compatible provider，将对话请求转发到 OCG Manager。',
    steps: [
      '打开 Roo Code 配置，将 API Provider 选择为 OpenAI Compatible。',
      '填写下方 Base URL、Key 和模型 ID。',
      '发送一条测试任务，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: ['Agent 模式依赖模型的工具调用能力；仅聊天成功不代表所有模式均可用。'],
    snippets: (context) => [
      keyedSnippet(
        context,
        t('Provider 参数'),
        'text',
        (key) => `Base URL: ${context.apiBaseUrl}\nAPI Key: ${key}\nModel ID: minimax-m3`,
      ),
    ],
  },
  {
    id: 'continue',
    name: 'Continue',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://docs.continue.dev/customize/model-providers/top-level/openai',
    summary: '在 Continue YAML 配置中添加 OpenAI provider，并明确关闭 Responses API。',
    steps: [
      '打开 Continue 用户级 YAML 配置，将下面的模型项合并到 models。',
      '保持 provider 为 openai、apiBase 使用 /v1 地址、useResponsesApi 为 false。',
      '选择 OCG Manager 模型发送测试消息，再到请求日志确认成功记录。',
    ],
    notes: ['useResponsesApi: false 用于明确走 Chat Completions 兼容路径。'],
    snippets: (context) => [
      keyedSnippet(
        context,
        'Continue YAML',
        'yaml',
        (key) =>
          `models:\n  - name: MiniMax M3 (OCG)\n    provider: openai\n    model: minimax-m3\n    apiBase: ${JSON.stringify(context.apiBaseUrl)}\n    apiKey: ${JSON.stringify(key)}\n    useResponsesApi: false`,
      ),
    ],
  },
  {
    id: 'chatbox',
    name: 'Chatbox',
    protocol: 'OpenAI Chat Completions',
    officialUrl: 'https://docs.chatboxai.app/en/guides/providers',
    summary: '新增 OpenAI API 类型提供商，API Host 使用 OCG Manager 根地址。',
    steps: [
      '打开设置 → 模型提供方，选择 OpenAI API 或兼容提供方。',
      '填写下方 API Host、Key 和模型 ID，保留默认的 /v1/chat/completions 路径。',
      '发送一条测试消息，再到 OCG Manager 的请求日志确认成功记录。',
    ],
    notes: ['API Host 使用不带 /v1 的根地址，避免形成重复路径。'],
    snippets: (context) => [
      keyedSnippet(
        context,
        t('Provider 参数'),
        'text',
        (key) => `API Host: ${context.rootUrl}\nAPI Key: ${key}\nModel ID: minimax-m3`,
      ),
    ],
  },
];

export function isApplicationId(value: string | null | undefined): value is ApplicationId {
  return typeof value === 'string' && APPLICATION_IDS.some((id) => id === value);
}
