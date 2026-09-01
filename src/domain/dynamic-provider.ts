import type { ProviderCatalogEntry } from "../api/providers.ts";
import { customEndpointUrlIssue } from "./custom-account.ts";

export const DYNAMIC_PROVIDER_MODEL_SOURCE = "dynamic_provider";

export type DynamicAuthKind = "bearer" | "x-api-key" | "none";
export type DynamicUpstreamProtocol = "chat_completions" | "responses" | "messages";

export interface DynamicProviderMapping {
  public_model: string;
  upstream_model: string;
}

export interface DynamicProviderDraft {
  name: string;
  endpoint_url: string;
  upstream_protocol: DynamicUpstreamProtocol | "";
  auth_kind: DynamicAuthKind | "";
  models: DynamicProviderMapping[];
  account_name: string;
  notes: string;
  key: string;
}

export type DynamicProviderSource = "builtin" | "user-defined";

export type DynamicProviderDraftError =
  | "missing_name"
  | "missing_endpoint_url"
  | "invalid_endpoint_url"
  | "endpoint_url_not_http"
  | "endpoint_url_with_credentials"
  | "missing_protocol"
  | "missing_auth_kind"
  | "missing_mappings"
  | "duplicate_public_model"
  | "missing_public_model"
  | "missing_upstream_model"
  | "public_model_too_long"
  | "public_model_has_control_character"
  | "upstream_model_too_long"
  | "upstream_model_has_control_character"
  | "missing_key"
  | "missing_replacement_key";

export const DYNAMIC_PROVIDER_DRAFT_ERROR_KEYS = {
  missing_name: "请填写供应商名称",
  missing_endpoint_url: "请填写 API 地址",
  invalid_endpoint_url: "Endpoint 格式无效",
  endpoint_url_not_http: "Endpoint 必须是 http:// 或 https:// URL",
  endpoint_url_with_credentials: "Endpoint 不能包含用户名或密码",
  missing_protocol: "请选择上游协议",
  missing_auth_kind: "请选择鉴权方式",
  missing_mappings: "请至少添加一个完整模型映射",
  duplicate_public_model: "对外模型名不能重复",
  missing_public_model: "请填写对外模型名",
  missing_upstream_model: "请填写上游模型 ID",
  public_model_too_long: "对外模型名最多 200 个字符",
  public_model_has_control_character: "对外模型名不能包含控制字符",
  upstream_model_too_long: "上游模型 ID 最多 200 个字符",
  upstream_model_has_control_character: "上游模型 ID 不能包含控制字符",
  missing_key: "请填写 API Key",
  missing_replacement_key: "从无鉴权改为需要 Key 时必须填写替换 Key",
} as const satisfies Record<DynamicProviderDraftError, string>;

export const DYNAMIC_AUTH_KINDS: readonly DynamicAuthKind[] = ["bearer", "x-api-key", "none"];
export const DYNAMIC_PROTOCOLS: readonly DynamicUpstreamProtocol[] = [
  "chat_completions",
  "responses",
  "messages",
];

export function isDynamicCatalogEntry(
  entry: Pick<ProviderCatalogEntry, "model_source">,
): boolean {
  return entry.model_source === DYNAMIC_PROVIDER_MODEL_SOURCE;
}

export function providerSourceLabel(
  entry: Pick<ProviderCatalogEntry, "model_source"> | null | undefined,
): DynamicProviderSource {
  return entry && isDynamicCatalogEntry(entry) ? "user-defined" : "builtin";
}

export function dynamicAuthRequiresKey(authKind: DynamicAuthKind | ""): boolean {
  return authKind === "bearer" || authKind === "x-api-key";
}

export function emptyDynamicProviderDraft(): DynamicProviderDraft {
  return {
    name: "",
    endpoint_url: "",
    upstream_protocol: "chat_completions",
    auth_kind: "bearer",
    models: [{ public_model: "", upstream_model: "" }],
    account_name: "",
    notes: "",
    key: "",
  };
}

export function sanitizeDynamicProviderDraft(draft: DynamicProviderDraft): DynamicProviderDraft {
  return {
    ...draft,
    key: "",
  };
}

function hasControlCharacter(value: string): boolean {
  return [...value].some((char) => {
    const code = char.codePointAt(0) ?? 0;
    return code < 32 || code === 127;
  });
}

export function normalizeDynamicMappings(
  mappings: readonly DynamicProviderMapping[],
): DynamicProviderMapping[] | DynamicProviderDraftError {
  const normalized: DynamicProviderMapping[] = [];
  const seen = new Set<string>();
  for (const mapping of mappings) {
    const publicModel = mapping.public_model.trim();
    const upstreamModel = mapping.upstream_model.trim();
    if (!publicModel && !upstreamModel) continue;
    if (!publicModel) return "missing_public_model";
    if (!upstreamModel) return "missing_upstream_model";
    if (publicModel.length > 200) return "public_model_too_long";
    if (hasControlCharacter(publicModel)) return "public_model_has_control_character";
    if (upstreamModel.length > 200) return "upstream_model_too_long";
    if (hasControlCharacter(upstreamModel)) return "upstream_model_has_control_character";
    const key = publicModel.toLocaleLowerCase();
    if (seen.has(key)) return "duplicate_public_model";
    seen.add(key);
    normalized.push({ public_model: publicModel, upstream_model: upstreamModel });
  }
  if (normalized.length === 0) return "missing_mappings";
  return normalized;
}

export function validateDynamicProviderDraft(
  draft: DynamicProviderDraft,
  options: { mode: "create" | "edit"; previousAuthKind?: DynamicAuthKind | "" } = { mode: "create" },
): DynamicProviderDraftError | null {
  if (!draft.name.trim()) return "missing_name";
  if (!draft.endpoint_url.trim()) return "missing_endpoint_url";
  const endpointIssue = customEndpointUrlIssue(draft.endpoint_url);
  if (endpointIssue === "malformed") return "invalid_endpoint_url";
  if (endpointIssue === "not_http") return "endpoint_url_not_http";
  if (endpointIssue === "with_credentials") return "endpoint_url_with_credentials";
  if (draft.upstream_protocol !== "chat_completions"
    && draft.upstream_protocol !== "responses"
    && draft.upstream_protocol !== "messages") {
    return "missing_protocol";
  }
  if (draft.auth_kind !== "bearer" && draft.auth_kind !== "x-api-key" && draft.auth_kind !== "none") {
    return "missing_auth_kind";
  }
  const mappings = normalizeDynamicMappings(draft.models);
  if (typeof mappings === "string") return mappings;
  if (options.mode === "create" && dynamicAuthRequiresKey(draft.auth_kind) && !draft.key.trim()) {
    return "missing_key";
  }
  if (
    options.mode === "edit"
    && options.previousAuthKind === "none"
    && dynamicAuthRequiresKey(draft.auth_kind)
    && !draft.key.trim()
  ) {
    return "missing_replacement_key";
  }
  return null;
}

export function buildDynamicProviderCreateBody(draft: DynamicProviderDraft): {
  name: string;
  endpointUrl: string;
  upstreamProtocol: DynamicUpstreamProtocol;
  authKind: DynamicAuthKind;
  models: Array<{ publicModel: string; upstreamModel: string }>;
  accountName?: string;
  notes?: string;
  key?: string;
} {
  const error = validateDynamicProviderDraft(draft, { mode: "create" });
  if (error) throw new Error(error);
  const models = normalizeDynamicMappings(draft.models);
  if (typeof models === "string") throw new Error(models);
  const body: ReturnType<typeof buildDynamicProviderCreateBody> = {
    name: draft.name.trim(),
    endpointUrl: draft.endpoint_url.trim(),
    upstreamProtocol: draft.upstream_protocol as DynamicUpstreamProtocol,
    authKind: draft.auth_kind as DynamicAuthKind,
    models: models.map((mapping) => ({
      publicModel: mapping.public_model,
      upstreamModel: mapping.upstream_model,
    })),
  };
  const accountName = draft.account_name.trim();
  if (accountName) body.accountName = accountName;
  const notes = draft.notes.trim();
  if (notes) body.notes = notes;
  if (dynamicAuthRequiresKey(draft.auth_kind)) body.key = draft.key.trim();
  return body;
}

export function buildDynamicProviderUpdateBody(
  draft: DynamicProviderDraft,
  previousAuthKind: DynamicAuthKind | "",
): {
  name: string;
  endpointUrl: string;
  upstreamProtocol: DynamicUpstreamProtocol;
  authKind: DynamicAuthKind;
  models: Array<{ publicModel: string; upstreamModel: string }>;
  key?: string;
} {
  const error = validateDynamicProviderDraft(draft, { mode: "edit", previousAuthKind });
  if (error) throw new Error(error);
  const models = normalizeDynamicMappings(draft.models);
  if (typeof models === "string") throw new Error(models);
  const body: ReturnType<typeof buildDynamicProviderUpdateBody> = {
    name: draft.name.trim(),
    endpointUrl: draft.endpoint_url.trim(),
    upstreamProtocol: draft.upstream_protocol as DynamicUpstreamProtocol,
    authKind: draft.auth_kind as DynamicAuthKind,
    models: models.map((mapping) => ({
      publicModel: mapping.public_model,
      upstreamModel: mapping.upstream_model,
    })),
  };
  if (draft.key.trim()) body.key = draft.key.trim();
  return body;
}

export function createBodyOmitsSecretAfterBuild(body: { key?: string }): boolean {
  return !("key" in body) || body.key === undefined;
}

export function omitSecretFromRecord(value: Record<string, unknown>): Record<string, unknown> {
  const next = { ...value };
  delete next.key;
  delete next.apiKey;
  return next;
}

export const DYNAMIC_PAID_TEST_WARNING_KEY = "真实测试会消耗上游额度或产生费用。确定继续？" as const;

export type DynamicProviderUiAction = "save" | "discover" | "test" | "delete";

/** Enter in the Provider form saves. Paid tests and deletes stay behind confirmation. */
export function dynamicProviderFormEnterAction(): "save" {
  return "save";
}

export function dynamicProviderActionNeedsConfirm(action: DynamicProviderUiAction): boolean {
  return action === "test" || action === "delete";
}
