import type { ProviderCatalogEntry } from "../api/providers.ts";
import type { MessageKey } from "../i18n/index.ts";
import type { PlanDefinition } from "./plans.ts";
import {
  PLAN_DEFINITIONS,
  dynamicPlanDefinition,
  findCatalogEntry,
  planFamilyLabel,
  planCreateDisabledReason,
} from "./plans.ts";
import { isDynamicCatalogEntry } from "./dynamic-provider.ts";

/**
 * Plan-option list for the Add Account chooser. Backend-owned singletons
 * (Zen Free) are omitted: they are not created here. Remaining families stay
 * visible so unavailable choices still explain why they cannot be created.
 * Unroutable-but-creatable families appear as drafts instead of implying they
 * will route.
 */

export interface PlanOption {
  optionId: string;
  plan: PlanDefinition;
  label: string;
  source: "builtin" | "user-defined";
  disabled: boolean;
  disabledReason: MessageKey | "";
  /** Honest copy for selectable-but-not-yet-routable families. */
  creationHint: MessageKey | "";
  managed: boolean;
}

export type PlanChooserGroupId = "available" | "draft" | "unavailable";

export interface PlanChooserGroup {
  id: PlanChooserGroupId;
  label: MessageKey;
  options: PlanOption[];
}

const GROUP_ORDER: readonly PlanChooserGroupId[] = ["available", "draft", "unavailable"];

const GROUP_LABEL: Record<PlanChooserGroupId, MessageKey> = {
  available: "可添加",
  draft: "草稿方案",
  unavailable: "暂不可用",
};

/**
 * Human-readable hint shown for selectable families whose post-create state
 * needs honest copy. GOAT is live without a Key-verification gate; Custom is
 * enabled by default and exposes account-scoped connection tests afterwards.
 */
function planCreationHint(
  plan: PlanDefinition,
  _catalog: readonly ProviderCatalogEntry[] | null | undefined,
): MessageKey | "" {
  if (plan.id === "custom-endpoint") return "创建后默认启用；可随时通过账号卡片测试连接。";
  return "";
}

/** True when the family's provider is routable according to the catalog. */
function planFamilyRoutable(
  plan: PlanDefinition,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): boolean {
  if (!catalog?.length) return false;
  return findCatalogEntry(catalog, plan.provider_id)?.routable === true;
}

function builtinOption(
  plan: PlanDefinition,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanOption {
  const reason = planCreateDisabledReason(plan, catalog);
  return {
    optionId: plan.id,
    plan,
    label: planFamilyLabel(plan, catalog),
    source: "builtin",
    disabled: Boolean(reason),
    disabledReason: reason ?? "",
    creationHint: reason ? "" : planCreationHint(plan, catalog),
    managed: !reason && plan.managed_registration,
  };
}

function dynamicOption(entry: ProviderCatalogEntry): PlanOption {
  const plan = dynamicPlanDefinition(entry);
  const blocked = entry.singleton || entry.creation_availability !== "available";
  const noAuthSingleton = entry.singleton || entry.credential_kind === "none";
  return {
    optionId: entry.provider_id,
    plan,
    label: entry.display_name || entry.provider_id,
    source: "user-defined",
    disabled: blocked,
    disabledReason: blocked
      ? (noAuthSingleton ? "无鉴权供应商只能有一个账号。" : "该方案暂不可用")
      : "",
    creationHint: blocked ? "" : "账号不拥有 Endpoint、协议或模型映射。",
    managed: false,
  };
}

export function buildPlanOptions(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanOption[] {
  const builtin = PLAN_DEFINITIONS.filter((plan) => !plan.singleton).map((plan) => (
    builtinOption(plan, catalog)
  ));
  const dynamic = (catalog ?? [])
    .filter(isDynamicCatalogEntry)
    .map(dynamicOption);
  return [...builtin, ...dynamic];
}

export function planChooserGroupId(
  option: PlanOption,
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanChooserGroupId {
  if (option.disabled) return "unavailable";
  if (!catalog?.length) return "available";
  return planFamilyRoutable(option.plan, catalog) ? "available" : "draft";
}

export function buildPlanChooserGroups(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
): PlanChooserGroup[] {
  const buckets: Record<PlanChooserGroupId, PlanOption[]> = {
    available: [],
    draft: [],
    unavailable: [],
  };
  for (const option of buildPlanOptions(catalog)) {
    buckets[planChooserGroupId(option, catalog)].push(option);
  }
  return GROUP_ORDER
    .filter((id) => buckets[id].length > 0)
    .map((id) => ({ id, label: GROUP_LABEL[id], options: buckets[id] }));
}
