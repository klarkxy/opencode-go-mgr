import type { ProviderCatalogEntry } from "../api/providers.ts";
import { PROVIDER_FAMILIES, type ProviderFamily } from "./provider-families.ts";
import { PROVIDER_PRESETS } from "./provider-presets.ts";

/**
 * Presentation logic for the Providers page: the rail lists catalog entries
 * (built-in seeds plus every created dynamic Provider) grouped by offering,
 * and the add flow is a small browse → form state machine mirrored in the URL.
 */

const FAMILIES_BY_ID: ReadonlyMap<string, ProviderFamily> = new Map(
  PROVIDER_FAMILIES.map((family) => [family.id, family]),
);

/** Built-in provider ids whose brand family id differs from the provider id. */
const PROVIDER_FAMILY_ALIASES: Readonly<Record<string, string>> = {
  kimi: "moonshot",
};

const FALLBACK_FAMILY_TINT = "#5F6068";

/**
 * Brand for a rail row or detail header. A known vendor family wins (logo or
 * tinted monogram); everything else gets a neutral monogram carrying the
 * catalog display family. Identity is never inferred from endpoint URLs.
 */
export function catalogEntryFamily(
  entry: Pick<ProviderCatalogEntry, "provider_id" | "display_family" | "display_name">,
): ProviderFamily {
  const alias = PROVIDER_FAMILY_ALIASES[entry.provider_id] ?? entry.provider_id;
  const known = FAMILIES_BY_ID.get(alias);
  if (known) return known;
  return {
    id: entry.provider_id,
    label: entry.display_family.trim() || entry.display_name,
    tint: FALLBACK_FAMILY_TINT,
  };
}

/**
 * Offering groups for the rail. Catalog order is preserved within each group:
 * built-in seeds keep their curated order and dynamic Providers follow by
 * creation time, exactly as the backend emits them.
 */
export function groupCatalogEntriesByOffering(
  entries: readonly ProviderCatalogEntry[],
): { plan: ProviderCatalogEntry[]; api: ProviderCatalogEntry[] } {
  return {
    plan: entries.filter((entry) => entry.offering === "plan"),
    api: entries.filter((entry) => entry.offering !== "plan"),
  };
}

/** Case-insensitive rail filter over the display name and the provider id. */
export function filterCatalogEntries(
  entries: readonly ProviderCatalogEntry[],
  query: string,
): ProviderCatalogEntry[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [...entries];
  return entries.filter((entry) => (
    entry.display_name.toLocaleLowerCase().includes(needle)
    || entry.provider_id.toLocaleLowerCase().includes(needle)
  ));
}

/** Query value representing the manual (preset-less) embedded form. */
export const MANUAL_PRESET_QUERY_VALUE = "manual";

export type ProviderAddStage =
  | { stage: "browse" }
  | { stage: "form"; presetId: string | null };

/**
 * Add-flow stage from URL parameters. An unknown preset id degrades to the
 * preset browser instead of a blank form.
 */
export function providerAddStageFromQuery(
  add: boolean,
  preset: string | null,
): ProviderAddStage | null {
  if (!add) return null;
  if (!preset) return { stage: "browse" };
  if (preset === MANUAL_PRESET_QUERY_VALUE) return { stage: "form", presetId: null };
  if (PROVIDER_PRESETS.some((entry) => entry.id === preset)) {
    return { stage: "form", presetId: preset };
  }
  return { stage: "browse" };
}

/** URL parameters mirroring an add-flow stage (see {@link providerAddStageFromQuery}). */
export function providerAddStageToQuery(
  stage: ProviderAddStage,
): { add: boolean; preset?: string } {
  if (stage.stage === "browse") return { add: true };
  return { add: true, preset: stage.presetId ?? MANUAL_PRESET_QUERY_VALUE };
}
