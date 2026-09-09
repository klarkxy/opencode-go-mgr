import type { ProviderCatalogEntry } from "../api/providers.ts";
import type { MessageKey } from "../i18n/index.ts";
import {
  splitPlanOptionsByOffering,
  type PlanOption,
} from "./account-plan-options.ts";
import {
  buildPlatformKindOptions,
  type PlatformKindOption,
} from "./platform-accounts.ts";
import {
  PROVIDER_PRESETS,
  filterProviderPresets,
  groupProviderPresetsByOffering,
  type ProviderPreset,
} from "./provider-presets.ts";

/**
 * Presentation logic for the Add Account chooser. Pure helpers only; the
 * component keeps Vue state (selection, busy guards) and maps icon keys to
 * icon components. Group labels are literal product terms, not translated
 * message keys.
 */

/**
 * Preset choices are deliberate new instances: their ids carry a prefix so
 * they can never be silently matched to a saved user-defined Provider by
 * display name.
 */
export interface PresetChooserOption {
  optionId: string;
  preset: ProviderPreset;
  label: string;
}

export type ChooserOption = PlanOption | PresetChooserOption | PlatformKindOption;

/** Exactly two user-visible groups, Plan above API. */
export interface ChooserGroup {
  id: "plan" | "api";
  label: "Plan" | "API";
  options: ChooserOption[];
}

export function toPresetChooserOption(preset: ProviderPreset): PresetChooserOption {
  return { optionId: `preset:${preset.id}`, preset, label: preset.name };
}

export function chooserOptionKind(option: ChooserOption): "plan" | "preset" | "platform" {
  if ("plan" in option) return "plan";
  if ("preset" in option) return "preset";
  return "platform";
}

/**
 * Visible groups for the rail: Custom API heads the API group, account-owned
 * user-defined Providers follow their persisted preset's offering via
 * `dynamicPresetIds`, API presets and platform kinds trail. The preset search
 * query filters preset entries only; plan and platform entries always show.
 */
export function buildChooserGroups(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  dynamicPresetIds: ReadonlyMap<string, string | null> | null | undefined,
  query: string,
): ChooserGroup[] {
  const split = splitPlanOptionsByOffering(catalog, dynamicPresetIds);
  const presets = groupProviderPresetsByOffering(filterProviderPresets(PROVIDER_PRESETS, query));
  return [
    {
      id: "plan",
      label: "Plan",
      options: [...split.plan, ...presets.plan.map(toPresetChooserOption)],
    },
    {
      id: "api",
      label: "API",
      options: [
        ...split.api,
        ...presets.api.map(toPresetChooserOption),
        ...buildPlatformKindOptions(),
      ],
    },
  ];
}

/**
 * The unfiltered option universe. Selection validity and the default
 * selection use this full list so typing in the preset search never blanks
 * the selected detail.
 */
export function chooserUniverse(
  catalog: readonly ProviderCatalogEntry[] | null | undefined,
  dynamicPresetIds: ReadonlyMap<string, string | null> | null | undefined,
): ChooserOption[] {
  const split = splitPlanOptionsByOffering(catalog, dynamicPresetIds);
  return [
    ...split.plan,
    ...split.api,
    ...PROVIDER_PRESETS.map(toPresetChooserOption),
    ...buildPlatformKindOptions(),
  ];
}

/** Visible (filtered) options in rail order; arrow-key navigation follows it. */
export function visibleChooserOptions(groups: readonly ChooserGroup[]): ChooserOption[] {
  return groups.flatMap((group) => group.options);
}

export function isChooserOptionDisabled(option: ChooserOption): boolean {
  return "disabled" in option && Boolean(option.disabled);
}

export function isValidChooserOption(
  options: readonly ChooserOption[],
  optionId: string,
): boolean {
  return options.some((option) => option.optionId === optionId);
}

/** First selectable option, or the first option when every one is disabled. */
export function defaultChooserOptionId(options: readonly ChooserOption[]): string {
  return options.find((option) => !isChooserOptionDisabled(option))?.optionId
    ?? options[0]?.optionId
    ?? "";
}

/** True when a non-empty preset search matched no preset at all. */
export function chooserPresetSearchMiss(query: string): boolean {
  return Boolean(query.trim()) && filterProviderPresets(PROVIDER_PRESETS, query).length === 0;
}

export interface ChooserSelectChild {
  label: string;
  value: string;
  /** Naive UI option objects carry an open index signature. */
  [key: string]: unknown;
}

/** Structurally compatible with Naive UI SelectGroupOption. */
export interface ChooserSelectGroup {
  type: "group";
  key: string;
  label: string;
  children: ChooserSelectChild[];
  [key: string]: unknown;
}

/** Narrow-screen fallback: the same groups rendered as a grouped select. */
export function chooserSelectOptions(
  groups: readonly ChooserGroup[],
  userDefinedLabel: string,
): ChooserSelectGroup[] {
  return groups.map((group) => ({
    type: "group" as const,
    key: group.id,
    label: group.label,
    children: group.options.map((option) => ({
      label: "source" in option && option.source === "user-defined"
        ? `${option.label} · ${userDefinedLabel}`
        : option.label,
      value: option.optionId,
    })),
  }));
}

/** Component-side icon map key; the component owns the actual components. */
export function chooserOptionIconKey(option: ChooserOption): string {
  if ("plan" in option) return option.plan.id;
  if ("preset" in option) return "api";
  return "database";
}

export interface ChooserDetail {
  kind: "plan" | "preset" | "platform";
  iconKey: string;
  title: string;
  tag: { label: MessageKey; type: "warning" | "default" } | null;
  links: { docsUrl: string; websiteUrl: string } | null;
}

/** Unified detail-header description for whichever option is selected. */
export function describeChooserSelection(option: ChooserOption): ChooserDetail {
  if ("preset" in option) {
    return {
      kind: "preset",
      iconKey: "api",
      title: option.preset.name,
      tag: { label: "供应商预设", type: "default" },
      links: { docsUrl: option.preset.docsUrl, websiteUrl: option.preset.websiteUrl },
    };
  }
  if (!("plan" in option)) {
    return { kind: "platform", iconKey: "database", title: option.label, tag: null, links: null };
  }
  let tag: ChooserDetail["tag"] = null;
  if (option.source === "user-defined") tag = { label: "用户定义", type: "default" };
  else if (option.plan.kind === "custom") tag = { label: "自定义端点", type: "default" };
  else if (option.plan.id === "dynamic-http") tag = { label: "用户定义", type: "default" };
  return { kind: "plan", iconKey: option.plan.id, title: option.label, tag, links: null };
}
