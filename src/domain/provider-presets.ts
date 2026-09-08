import presetsJson from "../../resources/provider-presets.json" with { type: "json" };
import {
  emptyDynamicProviderDraft,
  type DynamicAuthKind,
  type DynamicProviderDraft,
  type DynamicProviderMapping,
  type DynamicUpstreamProtocol,
} from "./dynamic-provider.ts";

export type ProviderPresetCategory = "official" | "aggregator";
export type ProviderPresetAuthKind = Extract<DynamicAuthKind, "bearer" | "x-api-key">;

export interface ProviderPreset {
  id: string;
  name: string;
  category: ProviderPresetCategory;
  /** Full inference URL, or "" when the endpoint is customer-specific. */
  endpointUrl: string;
  protocol: DynamicUpstreamProtocol;
  authKind: ProviderPresetAuthKind;
  docsUrl: string;
  websiteUrl: string;
  note: { en: string; zh: string };
  endpointPlaceholder?: string;
  /** False means no model-discovery interface is configured for this preset. */
  modelDiscovery?: boolean;
}

const PRESET_CATEGORIES: readonly ProviderPresetCategory[] = ["official", "aggregator"];
const PRESET_PROTOCOLS: readonly DynamicUpstreamProtocol[] = [
  "chat_completions",
  "responses",
  "messages",
];
const PRESET_AUTH_KINDS: readonly ProviderPresetAuthKind[] = ["bearer", "x-api-key"];

function isHttpUrl(value: unknown): value is string {
  if (typeof value !== "string" || !value) return false;
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Lists every way a raw row violates the frozen preset contract; an empty
 * result means the row is valid. Used by tests and by the lenient parser.
 */
export function providerPresetShapeIssues(raw: unknown, index = 0): string[] {
  const issues: string[] = [];
  const where = `row ${index}`;
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return [`${where}: not an object`];
  }
  const row = raw as Record<string, unknown>;
  if (typeof row.id !== "string" || !row.id.trim()) issues.push(`${where}: id missing`);
  if (typeof row.name !== "string" || !row.name.trim()) issues.push(`${where}: name missing`);
  if (!PRESET_CATEGORIES.includes(row.category as ProviderPresetCategory)) {
    issues.push(`${where}: category must be official or aggregator`);
  }
  if (typeof row.endpointUrl !== "string") {
    issues.push(`${where}: endpointUrl must be a string (empty allowed)`);
  } else if (row.endpointUrl && !isHttpUrl(row.endpointUrl)) {
    issues.push(`${where}: endpointUrl is not an http(s) URL`);
  }
  if (!PRESET_PROTOCOLS.includes(row.protocol as DynamicUpstreamProtocol)) {
    issues.push(`${where}: protocol must be chat_completions, responses, or messages`);
  }
  if (!PRESET_AUTH_KINDS.includes(row.authKind as ProviderPresetAuthKind)) {
    issues.push(`${where}: authKind must be bearer or x-api-key`);
  }
  if (!isHttpUrl(row.docsUrl)) issues.push(`${where}: docsUrl is not an http(s) URL`);
  if (!isHttpUrl(row.websiteUrl)) issues.push(`${where}: websiteUrl is not an http(s) URL`);
  const note = row.note as Record<string, unknown> | undefined;
  if (typeof note !== "object" || note === null
    || typeof note.en !== "string" || !note.en.trim()
    || typeof note.zh !== "string" || !note.zh.trim()) {
    issues.push(`${where}: note needs non-empty en and zh`);
  }
  if (row.endpointPlaceholder !== undefined
    && (typeof row.endpointPlaceholder !== "string" || !row.endpointPlaceholder.trim())) {
    issues.push(`${where}: endpointPlaceholder must be a non-empty string when present`);
  }
  if (row.modelDiscovery !== undefined && typeof row.modelDiscovery !== "boolean") {
    issues.push(`${where}: modelDiscovery must be a boolean when present`);
  }
  return issues;
}

/** Keeps only contract-valid rows so a bad entry cannot break the dashboard. */
export function parseProviderPresets(raw: unknown): ProviderPreset[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const presets: ProviderPreset[] = [];
  for (const [index, row] of raw.entries()) {
    if (providerPresetShapeIssues(row, index).length > 0) continue;
    const preset = row as ProviderPreset;
    if (seen.has(preset.id)) continue;
    seen.add(preset.id);
    presets.push(preset);
  }
  return presets;
}

export const PROVIDER_PRESETS: readonly ProviderPreset[] = Object.freeze(
  parseProviderPresets(presetsJson),
);

export function groupProviderPresets(
  presets: readonly ProviderPreset[],
): { official: ProviderPreset[]; aggregator: ProviderPreset[] } {
  return {
    official: presets.filter((preset) => preset.category === "official"),
    aggregator: presets.filter((preset) => preset.category === "aggregator"),
  };
}

export function filterProviderPresets(
  presets: readonly ProviderPreset[],
  query: string,
): ProviderPreset[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [...presets];
  return presets.filter((preset) => (
    preset.name.toLocaleLowerCase().includes(needle)
    || preset.id.toLocaleLowerCase().includes(needle)
  ));
}

/**
 * Builds the draft for a preset selection. Preset-filled fields (name,
 * endpoint, protocol, auth) always reset; the Key and model mappings are
 * cleared on every switch so a secret can never cross providers. The account
 * name and notes the user typed survive a switch.
 */
export function applyProviderPresetToDraft(
  current: DynamicProviderDraft,
  preset: ProviderPreset | null,
): DynamicProviderDraft {
  const base = emptyDynamicProviderDraft();
  return {
    ...base,
    name: preset ? preset.name : "",
    endpoint_url: preset ? preset.endpointUrl : "",
    upstream_protocol: preset ? preset.protocol : base.upstream_protocol,
    auth_kind: preset ? preset.authKind : base.auth_kind,
    // Persisted provenance follows the explicit picker choice: the exact
    // preset ID, or "" for a manual switch (create omits, update clears).
    preset_id: preset ? preset.id : "",
    account_name: current.account_name,
    notes: current.notes,
  };
}

/** A placeholder is display-only; it is never a usable endpoint value. */
export function providerPresetEndpointPlaceholder(preset: ProviderPreset): string {
  return preset.endpointPlaceholder ?? "";
}

export function providerPresetModelDiscoveryEnabled(preset: ProviderPreset): boolean {
  return preset.modelDiscovery !== false;
}

/**
 * Preset imports get a predictable `<preset-id>/<exact-id>` public name so
 * rows stay distinguishable on the aggregated Aliases view; manual imports
 * keep the plain upstream ID. The upstream ID is always the exact model ID.
 */
export function providerPresetImportPublicName(
  presetId: string | null,
  upstreamModelId: string,
): string {
  return presetId ? `${presetId}/${upstreamModelId}` : upstreamModelId;
}

export function providerPresetNote(preset: ProviderPreset, locale: string): string {
  return locale.toLocaleLowerCase().startsWith("zh") ? preset.note.zh : preset.note.en;
}

/**
 * Comparison identity for preset matching: origin plus the pathname without
 * trailing slashes. URLs carrying credentials, a query, or a fragment are
 * rejected outright instead of being silently normalized into an official
 * match; returns null for values that are not absolute http(s) URLs.
 */
export function normalizeProviderPresetEndpoint(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return null;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
  if (!parsed.hostname) return null;
  if (parsed.username || parsed.password || parsed.search || parsed.hash) return null;
  return `${parsed.origin}${parsed.pathname.replace(/\/+$/u, "")}`;
}

/**
 * Deterministic preset resolution for an existing draft: the normalized
 * endpoint must match exactly one preset's own endpoint with the same auth
 * kind. Sibling protocol paths are never derived or inferred, so a match
 * means the exact documented endpoint. Presets with a blank endpoint (Azure,
 * Bedrock) can never be inferred from a URL, and any ambiguity resolves to
 * null — the user picks manually instead of the UI guessing.
 */
export function resolveProviderPreset(
  endpointUrl: string,
  authKind: DynamicAuthKind | "",
  presets: readonly ProviderPreset[] = PROVIDER_PRESETS,
): ProviderPreset | null {
  const target = normalizeProviderPresetEndpoint(endpointUrl);
  if (!target) return null;
  if (authKind !== "bearer" && authKind !== "x-api-key") return null;
  const matches = presets.filter((preset) => (
    Boolean(preset.endpointUrl)
    && preset.authKind === authKind
    && normalizeProviderPresetEndpoint(preset.endpointUrl) === target
  ));
  return matches.length === 1 ? matches[0] ?? null : null;
}

/**
 * Naming-style evidence from existing mappings: when every non-empty public
 * name shares the same `<preset-id>/` prefix and that prefix is a known
 * preset, imports keep that prefix. Mixed, unknown, or absent prefixes return
 * null; a preset is never inferred from the provider's display name.
 */
export function inferMappingPresetPrefix(
  models: readonly Pick<DynamicProviderMapping, "public_model">[],
  presets: readonly ProviderPreset[] = PROVIDER_PRESETS,
): string | null {
  const knownIds = new Set(presets.map((preset) => preset.id));
  let found: string | null = null;
  let seen = 0;
  for (const model of models) {
    const name = model.public_model.trim();
    if (!name) continue;
    seen += 1;
    const slash = name.indexOf("/");
    if (slash <= 0) return null;
    const prefix = name.slice(0, slash);
    if (!knownIds.has(prefix)) return null;
    if (found === null) found = prefix;
    else if (found !== prefix) return null;
  }
  return seen > 0 ? found : null;
}

export interface EditPresetResolution {
  /** Persisted source-template preset, when the stored ID is a known preset. */
  template: ProviderPreset | null;
  /** Unique live endpoint+auth match; verified endpoint evidence only. */
  endpointMatch: ProviderPreset | null;
  /**
   * Naming preset for imports: the persisted template first, then a shared
   * mapping prefix. An endpoint-only match never prefixes previously
   * unprefixed manual mappings.
   */
  importPresetId: string | null;
  /** Preset gating model discovery: template first, then legacy inference. */
  discoveryPreset: ProviderPreset | null;
}

/**
 * Edit-mode preset context. Persisted provenance wins; legacy rows without a
 * stored ID fall back to the safe endpoint/prefix inference. Template metadata
 * and the verified endpoint claim stay separate so a modified custom URL is
 * never presented as the official endpoint.
 */
export function resolveEditPreset(
  persistedPresetId: string | null | undefined,
  endpointUrl: string,
  authKind: DynamicAuthKind | "",
  models: readonly Pick<DynamicProviderMapping, "public_model">[],
  presets: readonly ProviderPreset[] = PROVIDER_PRESETS,
): EditPresetResolution {
  const template = persistedPresetId
    ? presets.find((preset) => preset.id === persistedPresetId) ?? null
    : null;
  const endpointMatch = resolveProviderPreset(endpointUrl, authKind, presets);
  const prefix = inferMappingPresetPrefix(models, presets);
  const prefixPreset = prefix
    ? presets.find((preset) => preset.id === prefix) ?? null
    : null;
  return {
    template,
    endpointMatch,
    importPresetId: template?.id ?? prefix,
    discoveryPreset: template ?? endpointMatch ?? prefixPreset,
  };
}
