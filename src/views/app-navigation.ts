import type { MessageKey } from "../i18n/index.ts";

export const APP_NAVIGATION_GROUPS = {
  core: { key: "core" },
  extensions: { key: "extensions", label: "扩展" },
} as const satisfies Record<string, { key: string; label?: MessageKey }>;

export type AppNavigationGroup = keyof typeof APP_NAVIGATION_GROUPS;
export type AppNavigationIcon =
  | "dashboard"
  | "keys"
  | "accounts"
  | "providers"
  | "apps"
  | "logs"
  | "settings"
  | "cpa";

export interface AppNavigationItem {
  key: string;
  label: MessageKey;
  icon: AppNavigationIcon;
  group: AppNavigationGroup;
}

// This is the single navigation registration source for desktop, mobile, and
// the page title. Browser remains an overlay rather than a menu entry.
export const APP_NAVIGATION = [
  { key: "dashboard", label: "仪表盘", icon: "dashboard", group: "core" },
  { key: "keys", label: "接入 Key", icon: "keys", group: "core" },
  { key: "accounts", label: "账号", icon: "accounts", group: "core" },
  { key: "providers", label: "供应商", icon: "providers", group: "core" },
  { key: "apps", label: "应用", icon: "apps", group: "core" },
  { key: "logs", label: "日志", icon: "logs", group: "core" },
  { key: "settings", label: "设置", icon: "settings", group: "core" },
  { key: "cpa", label: "CPA", icon: "cpa", group: "extensions" },
] as const satisfies readonly AppNavigationItem[];

export type AppNavigationViewKey = (typeof APP_NAVIGATION)[number]["key"];
export type AppViewKey = AppNavigationViewKey | "browser";

export const APP_VIEW_KEYS: readonly AppViewKey[] = [
  ...APP_NAVIGATION.map(({ key }) => key),
  "browser",
];

export const CORE_APP_NAVIGATION = APP_NAVIGATION.filter(({ group }) => group === "core");
export const EXTENSION_APP_NAVIGATION = APP_NAVIGATION.filter(({ group }) => group === "extensions");

export const LEGACY_PRICING_VIEW = "pricing";
export const PROVIDERS_VIEW: AppViewKey = "providers";

const viewKeySet = new Set<string>(APP_VIEW_KEYS);

export interface ProviderScopeQuery {
  scope_kind?: string;
  scope_id?: string;
}

export function isLegacyPricingView(raw: string | null | undefined): boolean {
  return raw === LEGACY_PRICING_VIEW;
}

export function resolveAppViewKey(raw: string | null | undefined): AppViewKey {
  if (!raw) return "dashboard";
  if (isLegacyPricingView(raw) || raw === PROVIDERS_VIEW) return "providers";
  return viewKeySet.has(raw) ? raw as AppViewKey : "dashboard";
}

export function readProviderScopeQuery(search: string): {
  scope_kind: string | null;
  scope_id: string | null;
} {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  return {
    scope_kind: params.get("scope_kind"),
    scope_id: params.get("scope_id"),
  };
}

export function applyAppViewSearchParams(
  url: URL,
  view: AppViewKey,
  scope?: ProviderScopeQuery | null,
): URL {
  url.searchParams.set("view", view);
  if (view !== "accounts") url.searchParams.delete("account_id");
  if (view !== "providers") {
    url.searchParams.delete("scope_kind");
    url.searchParams.delete("scope_id");
    return url;
  }
  if (scope === undefined) return url;
  if (scope === null) {
    url.searchParams.delete("scope_kind");
    url.searchParams.delete("scope_id");
    return url;
  }
  if (scope.scope_kind) url.searchParams.set("scope_kind", scope.scope_kind);
  else url.searchParams.delete("scope_kind");
  if (scope.scope_id) url.searchParams.set("scope_id", scope.scope_id);
  else url.searchParams.delete("scope_id");
  return url;
}
