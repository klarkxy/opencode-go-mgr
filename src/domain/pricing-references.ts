/** Static official references. They are display-only and never debit local quota. */
export type GoatOfficialRate = number | "free" | null;

export interface GoatOfficialPricingRow {
  model: string;
  input: GoatOfficialRate;
  output: GoatOfficialRate;
  cacheRead: GoatOfficialRate;
  cacheWrite: GoatOfficialRate;
  quotaMultiplier: number | null;
}

/**
 * Command Code's GOAT plan table as published on the official plan page.
 * Rates are USD per 1M tokens. Multipliers mirror the provider snapshot's
 * normalized quota-debit presentation and remain display-only reference data.
 */
export const GOAT_PRICING_REFERENCE = {
  sourceUrl: "https://commandcode.ai/docs/plans/goat",
  pricingUrl: "https://commandcode.ai/docs/plans/goat#models-included",
  includedModelCount: 40,
  models: [
    { model: "GPT-5.6 Sol", input: 5, output: 30, cacheRead: 0.5, cacheWrite: 6.25, quotaMultiplier: 1 },
    { model: "GLM-5.2", input: 1.4, output: 4.4, cacheRead: 0.26, cacheWrite: null, quotaMultiplier: 1 },
    { model: "Tencent Hy3", input: 0.14, output: 0.58, cacheRead: 0.035, cacheWrite: null, quotaMultiplier: 1 },
    { model: "Qwen 3.8 27B", input: 0.4, output: 3, cacheRead: 0.04, cacheWrite: null, quotaMultiplier: 1 },
    { model: "DeepSeek V4 Flash (latest)", input: 0.22, output: 0.66, cacheRead: 0.007, cacheWrite: null, quotaMultiplier: 1.1666666666666667 },
    { model: "Kimi K2.7 Code", input: 0.95, output: 4, cacheRead: 0.19, cacheWrite: null, quotaMultiplier: 1.1666666666666667 },
    { model: "MiniMax M3", input: 0.3, output: 1.2, cacheRead: 0.06, cacheWrite: null, quotaMultiplier: 1.4893617021276595 },
    { model: "Gemini 3.7 Flash", input: 0.75, output: 3.75, cacheRead: 0.075, cacheWrite: 0.04167, quotaMultiplier: 1.75 },
    { model: "Qwen 3.7 Max", input: 2.5, output: 7.5, cacheRead: 0.5, cacheWrite: 3.13, quotaMultiplier: 2.121212121212121 },
    { model: "Qwen 3.7 Plus", input: 0.4, output: 1.6, cacheRead: 0.08, cacheWrite: 0.5, quotaMultiplier: 2.121212121212121 },
    { model: "Qwen 3.6 Plus", input: 0.5, output: 3, cacheRead: 0.1, cacheWrite: null, quotaMultiplier: 2.121212121212121 },
    { model: "MiMo V2.5", input: 0.14, output: 0.28, cacheRead: 0.0028, cacheWrite: null, quotaMultiplier: 2.3333333333333335 },
    { model: "GPT-5.6 Luna", input: 0.2, output: 1.2, cacheRead: 0.02, cacheWrite: 0.25, quotaMultiplier: 3.5 },
    { model: "Qwen 3.8 Max", input: 2, output: 6, cacheRead: 0.25, cacheWrite: 2.5, quotaMultiplier: 3.5 },
    { model: "DeepSeek V4 Pro (latest)", input: 0.66, output: 1.98, cacheRead: 0.022, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "MiMo V2.5 Pro", input: 0.435, output: 0.87, cacheRead: 0.0036, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "DeepSeek V4 Flash Vision (exp)", input: 0.22, output: 0.66, cacheRead: 0.01, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "GLM-5.3", input: 1.4, output: 4.4, cacheRead: 0.26, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Muse Spark 1.2", input: 1.25, output: 4.25, cacheRead: 0.15, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Muse Spark 1.2 Contributor", input: 0.1, output: 0.2, cacheRead: 0.002, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Kimi K3", input: 3, output: 15, cacheRead: 0.3, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Kimi K2.7 Code HighSpeed", input: 1.9, output: 8, cacheRead: 0.38, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Grok 4.5", input: 2, output: 6, cacheRead: 0.5, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Grok 4.6", input: 2, output: 6, cacheRead: 0.5, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "GLM-5.2 Fast", input: 3, output: 10.25, cacheRead: 0.5, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Inkling", input: 1, output: 4.05, cacheRead: 0.17, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Inkling Small", input: 0.5, output: 1.2, cacheRead: 0.1, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Step 3.7 Flash", input: 0.2, output: 1.15, cacheRead: 0.04, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Step 3.5 Flash", input: 0.1, output: 0.3, cacheRead: 0.02, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Nemotron 3 Ultra", input: 0.6, output: 2.4, cacheRead: 0.12, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Kimi K2.6", input: 0.95, output: 4, cacheRead: 0.16, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Kimi K2.5", input: 0.6, output: 3, cacheRead: 0.1, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "GLM-5.1", input: 1.4, output: 4.4, cacheRead: 0.26, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "GLM-5", input: 1, output: 3.2, cacheRead: 0.2, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Qwen 3.7 Flash", input: 0.03, output: 0.13, cacheRead: 0.006, cacheWrite: 0.038, quotaMultiplier: 3.5 },
    { model: "Qwen 3.6 Max Preview", input: 1.3, output: 7.8, cacheRead: 0.26, cacheWrite: 1.63, quotaMultiplier: 3.5 },
    { model: "MiniMax M2.7", input: 0.3, output: 1.2, cacheRead: 0.06, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "MiniMax M2.5", input: 0.3, output: 1.2, cacheRead: 0.03, cacheWrite: null, quotaMultiplier: 3.5 },
    { model: "Ox Alpha", input: "free", output: "free", cacheRead: "free", cacheWrite: null, quotaMultiplier: null },
    { model: "Laguna S 2.1", input: "free", output: "free", cacheRead: "free", cacheWrite: null, quotaMultiplier: null },
  ] satisfies readonly GoatOfficialPricingRow[],
} as const;
