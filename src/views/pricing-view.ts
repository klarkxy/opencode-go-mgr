export interface FormattedPricingRate {
  label: string;
  exact: string | null;
}

/**
 * Pricing rates are shown compactly in the table, while tiny non-zero values
 * retain an exact, inspectable representation for the tooltip.
 */
export function formatPricingRate(
  value: number | null,
  locale: string,
): FormattedPricingRate {
  if (value === null || !Number.isFinite(value)) return { label: "—", exact: null };
  if (value !== 0 && Math.abs(value) < 0.01) {
    const exact = new Intl.NumberFormat(locale, {
      style: "currency",
      currency: "USD",
      currencyDisplay: "narrowSymbol",
      minimumFractionDigits: 2,
      maximumFractionDigits: 8,
    }).format(value);
    return { label: value > 0 ? "<$0.01" : ">-$0.01", exact };
  }
  return {
    label: new Intl.NumberFormat(locale, {
      style: "currency",
      currency: "USD",
      currencyDisplay: "narrowSymbol",
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(value),
    exact: null,
  };
}

export function effectivePricingRate(
  rate: number | null,
  quotaMultiplier: number,
): number | null {
  return rate === null ? null : rate * quotaMultiplier;
}

export function officialPriceMultiplier(value: number | null | undefined): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : 1;
}

export function formatPricingMultiplier(value: number): string {
  return `×${new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 }).format(value)}`;
}
