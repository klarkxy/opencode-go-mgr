[简体中文](provider-presets.zh-CN.md)

# Official API presets

Open **Providers → New Provider**, then select a preset. OCG fills its name, full inference URL, upstream protocol and authentication. Enter that service's **Key**, fetch and select supported chat models or enter their exact IDs, then save. Providers needing a resource-specific URL leave the address empty until you supply it.

Presets create ordinary user-defined Providers through the existing atomic Dashboard V3 workflow. The saved Provider participates in account ordering, fallback, model routing and request logs. Its configuration stays editable. It does not add a separate adapter or automatically create an account before saving.

Models imported while a preset is selected receive a public name such as `openrouter/vendor/model`, while the exact upstream ID remains `vendor/model`. Both fields can be edited. Manual configuration imports retain the original public-name behavior.

Switching presets clears the draft Key and model mappings. Links lead to the operator's documentation and console without CC-Switch referral parameters. Discovery only changes the draft; a model test can be billable and requires the existing explicit confirmation. Check that each selected model supports the chosen upstream protocol. Image, audio, video and embedding-only models are outside this chat gateway's preset workflow.

## Discover, test and edit

Defaults were reviewed on **2026-09-08**: xAI uses Responses, and MiniMax CN/Global uses Messages with Bearer auth, following the current operator recommendation and REST reference. OpenAI/Azure/Bedrock retain Responses, Anthropic retains Messages, and the other presets retain their documented Chat-compatible defaults. Gemini's native Google API is outside these three upstream formats, so its preset deliberately uses the documented OpenAI compatibility surface. A documented compatibility option is not necessarily the operator's exclusive recommendation. Templates only initialize new drafts; saved choices are preserved.

MiniMax's official Chat and Messages URLs have different prefixes (`/v1` and `/anthropic/v1`). Configure the exact endpoint when overriding a model's protocol; the dashboard does not guess alternate paths. [Current MiniMax Messages authentication](https://platform.minimax.io/docs/api-reference/text-chat-anthropic) is Bearer, even though the wire format is Anthropic-compatible. See [protocol defaults](providers.md#protocol-defaults-and-connection-tests).

Fetch models only updates the draft candidate list. Empty and truncated results are shown explicitly; import only the models you select, then save. Discovery does not prove that every model accepts the selected protocol.

Choose a model and use **Test model**. The test uses its explicit protocol/endpoint override, or the supplier defaults when it inherits. A success requires a response matching the selected protocol, not just HTTP 200. It does not prove streaming, tools or other advanced features for that supplier.

Testing never changes routing or preference. **Save Provider** commits the configuration you edited. Custom API accounts test their configured account-owned protocol. Supply a temporary test Key when editing; this action does not read back or replace a saved Key. Linked platform Keys keep their managed endpoint constraints. Model overrides are available for user-defined Providers, not account-owned Custom endpoints.

The selected preset is retained when saving and reopening, including resource-specific addresses such as Azure. It preserves template hints, discovery restrictions and import naming; it does not certify an edited endpoint as official. Older entries without preset provenance use only unambiguous existing configuration evidence, and manually named models are not rewritten. Changing the endpoint, Key, model or protocol clears stale test results.

## Coverage and sources

Compared on **2026-09-08** against CC-Switch's Claude, Codex, Gemini, OpenCode, OpenClaw and Hermes [preset sources at `f3b18df`](https://github.com/farion1231/cc-switch/tree/f3b18df12007d0fd79fd8ad8d310880664015197/src/config). CC-Switch categories are discovery hints, not a trust decision: for example, Azure and xAI appear under third-party categories. The endpoint and authentication choices below were checked against operator documentation.

Each row is a ready configuration template, not a claim of authenticated live testing or account entitlement. New Providers remain unpriced: official quota, balance and pricing are not synchronized. Coding/Token Plan Keys and regional API Keys must match the selected endpoint; the upstream's supported-use restrictions still apply.

| Preset | Upstream | Authentication | Operator documentation |
| --- | --- | --- | --- |
| OpenAI API | responses | bearer | [API docs](https://developers.openai.com/api/reference/overview) |
| Anthropic API | messages | x-api-key | [API docs](https://platform.claude.com/docs/en/api/overview) |
| Google Gemini API | chat_completions | bearer | [API docs](https://ai.google.dev/gemini-api/docs/openai) |
| xAI (Grok) API | responses | bearer | [API docs](https://docs.x.ai/developers/quickstart) |
| Azure OpenAI v1 | responses | bearer | [API docs](https://learn.microsoft.com/en-us/azure/foundry/openai/api-version-lifecycle) |
| AWS Bedrock (API Key) | responses | bearer | [API docs](https://docs.aws.amazon.com/bedrock/latest/userguide/bedrock-mantle.html) |
| DeepSeek API | chat_completions | bearer | [API docs](https://api-docs.deepseek.com/) |
| Kimi / Moonshot API | chat_completions | bearer | [API docs](https://platform.kimi.com/docs/api/overview) |
| Zhipu GLM API | chat_completions | bearer | [API docs](https://docs.bigmodel.cn/cn/guide/develop/http/introduction) |
| Zhipu GLM Coding Plan | chat_completions | bearer | [API docs](https://docs.bigmodel.cn/cn/guide/develop/cursor) |
| Z.AI GLM API | chat_completions | bearer | [API docs](https://docs.z.ai/api-reference/introduction) |
| Z.AI GLM Coding Plan | chat_completions | bearer | [API docs](https://docs.z.ai/devpack/tool/others) |
| MiniMax API (CN) | messages | bearer | [API docs](https://platform.minimaxi.com/docs/api-reference/text-anthropic-api) |
| MiniMax API / Token Plan (Global) | messages | bearer | [API docs](https://platform.minimax.io/docs/api-reference/text-anthropic-api) |
| LongCat API | chat_completions | bearer | [API docs](https://longcat.chat/platform/docs/api/chat.html) |
| Tencent Hunyuan / TokenHub API | chat_completions | bearer | [API docs](https://cloud.tencent.com/document/product/1823/130078) |
| Tencent Token Plan (CN) | chat_completions | bearer | [API docs](https://cloud.tencent.com/document/product/1823/130119) |
| Tencent Token Plan (Global) | chat_completions | bearer | [API docs](https://www.tencentcloud.com/document/product/1300/81037) |
| Tencent Token Plan Enterprise Pro (CN) | chat_completions | bearer | [API docs](https://cloud.tencent.com/document/product/1823/130659) |
| Tencent Token Plan Enterprise Pro (Global) | chat_completions | bearer | [API docs](https://www.tencentcloud.com/document/product/1300/81489) |
| Tencent Token Plan Enterprise Lite (CN) | chat_completions | bearer | [API docs](https://cloud.tencent.com/document/product/1823/131173) |
| Tencent Token Plan Enterprise Lite (Global) | chat_completions | bearer | [API docs](https://www.tencentcloud.com/document/product/1300/81490) |
| Alibaba Bailian API (CN) | chat_completions | bearer | [API docs](https://help.aliyun.com/zh/model-studio/base-url) |
| Alibaba Bailian Coding Plan (CN) | chat_completions | bearer | [API docs](https://help.aliyun.com/zh/model-studio/coding-plan) |
| QwenCloud API (Global) | chat_completions | bearer | [API docs](https://www.alibabacloud.com/help/en/model-studio/base-url) |
| QwenCloud Coding Plan (Global) | chat_completions | bearer | [API docs](https://www.alibabacloud.com/help/en/model-studio/coding-plan) |
| QwenCloud Token Plan (Global) | chat_completions | bearer | [API docs](https://www.alibabacloud.com/help/en/model-studio/base-url) |
| Volcengine Ark / Doubao API | chat_completions | bearer | [API docs](https://www.volcengine.com/docs/ark/chat-api) |
| Volcengine Agent Plan | chat_completions | bearer | [API docs](https://www.volcengine.com/docs/82379/2160841) |
| Volcengine Coding Plan | chat_completions | bearer | [API docs](https://www.volcengine.com/docs/82379/1928261) |
| BytePlus Coding Plan | chat_completions | bearer | [API docs](https://docs.byteplus.com/en/docs/ModelArk/1928261) |
| Baidu Qianfan API | chat_completions | bearer | [API docs](https://cloud.baidu.com/doc/qianfan/s/Hmh4suq26) |
| Baidu Qianfan Coding Plan | chat_completions | bearer | [API docs](https://cloud.baidu.com/doc/qianfan/s/imlg0beiu) |
| Baidu Qianfan Token Plan (Team) | chat_completions | bearer | [API docs](https://cloud.baidu.com/doc/qianfan/s/smqeup7hm) |
| StepFun API (CN) | chat_completions | bearer | [API docs](https://platform.stepfun.com/docs/zh/api-reference/chat/chat-completion-create) |
| StepFun Step Plan (CN) | chat_completions | bearer | [API docs](https://platform.stepfun.com/docs/zh/step-plan/integrations/reasoning-api) |
| StepFun API (Global) | chat_completions | bearer | [API docs](https://platform.stepfun.ai/docs/en/api-reference/chat/chat-completion-create) |
| StepFun Step Plan (Global) | chat_completions | bearer | [API docs](https://platform.stepfun.ai/docs/en/step-plan/integrations/reasoning-api) |
| Xiaomi MiMo API | chat_completions | bearer | [API docs](https://mimo.mi.com/docs/zh-CN/quick-start/faq/api-integration) |
| Xiaomi MiMo Token Plan (CN) | chat_completions | bearer | [API docs](https://mimo.mi.com/docs/zh-CN/tokenplan/integration/tools-overview) |
| BaiLing / Ant Ling API | chat_completions | bearer | [API docs](https://developer.ant-ling.com/zh-CN/docs/api-reference/openai/) |
| KAT-Coder / StreamLake API | chat_completions | bearer | [API docs](https://www.streamlake.ai/document/DOC/mg6k6nlp8j6qxicx4c9) |
| KAT-Coder / StreamLake Coding Plan | chat_completions | bearer | [API docs](https://www.streamlake.ai/document/DOC/mg6k6nlp8j6qxicx4c9) |
| OpenRouter | chat_completions | bearer | [API docs](https://openrouter.ai/docs/quickstart) |
| SiliconFlow (CN) | chat_completions | bearer | [API docs](https://docs.siliconflow.cn/docs/userguide/quickstart) |
| SiliconFlow (Global) | chat_completions | bearer | [API docs](https://docs.siliconflow.com/en/userguide/quickstart) |
| NVIDIA API Catalog | chat_completions | bearer | [API docs](https://docs.api.nvidia.com/nim/reference/llm-apis) |
| ModelScope API Inference | chat_completions | bearer | [API docs](https://www.modelscope.cn/learn/434797) |
| PPIO | chat_completions | bearer | [API docs](https://ppio.com/docs/models/reference-llm-create-chat-completion) |
| Qiniu AI | chat_completions | bearer | [API docs](https://developer.qiniu.com/aitokenapi/13379/real-time-ai-interface-api) |
| Novita AI | chat_completions | bearer | [API docs](https://docs.novita.ai/api-reference/model-apis-llm-create-chat-completion) |
| Compshare ModelVerse | chat_completions | bearer | [API docs](https://compshare.cn/docs/modelverse/models/text_api/openai_compatible) |
| Compshare Coding Plan | chat_completions | bearer | [API docs](https://compshare.cn/docs/modelverse/codingfaq) |
| AtlasCloud Coding Plan | chat_completions | bearer | [API docs](https://www.atlascloud.ai/docs/coding-plan/api) |

KAT-Coder's full Chat URL and Bearer auth are derived from its official OpenAI-compatible client configuration (base URL plus the standard `/chat/completions` suffix). Ant Ling uses the current `api.ant-ling.com` official domain rather than CC-Switch's older `api.tbox.cn` entries.

**Unverified gap:** CC-Switch's Baidu personal Token Plan `/v2/tokenplan/personal` route could not be corroborated in the fetched official docs. It is not offered as a preset. The documented Qianfan general API, legacy Coding Plan and team Token Plan are included separately; do not use a personal Key on the team endpoint.

## Existing integrations and exclusions

- **OpenCode Go**, **Kimi Code CN** and **MiniMax CN Token Plan** retain their existing built-in routing and usage behavior. Select those existing Providers for the subscription workflow; the new Moonshot and MiniMax API presets cover the distinct API/region use cases.
- Azure uses the current v1 API with a resource URL and deployment name. It does not provision resources or refresh Entra ID tokens. Bedrock uses its official OpenAI-compatible API with an API Key; AWS AK/SK signing is not implemented.
- New API / One API / Sub2API distributors, subscription reverse proxies, referral-only relay listings and providers whose operator/API provenance could not be established are not included. A CC-Switch “aggregator” label alone is insufficient.
- The included aggregation/inference platforms have their own documented service: OpenRouter, SiliconFlow, NVIDIA, ModelScope, PPIO, Qiniu, Novita, Compshare and AtlasCloud. This is a selected set, not a blanket import of CC-Switch's relay catalog.
- Browser subscription login, GitHub Copilot OAuth, Codex OAuth and Grok OAuth are not API Key presets. Existing CPA integration is separate.

Models are deliberately not pinned to a copied CC-Switch snapshot. Use the provider's current catalog or console, retaining the exact upstream spelling. A missing model-list interface does not prevent saving an explicit mapping.

---

[Add a Provider](add-provider.md) · [Providers](providers.md) · [简体中文](provider-presets.zh-CN.md)
