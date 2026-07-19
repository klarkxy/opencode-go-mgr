export type TimePreset =
  | "all"
  | "last24h"
  | "last7d"
  | "last30d"
  | "thisMonth"
  | "lastMonth"
  | "custom";

export const timePresetValues = new Set<TimePreset>([
  "all",
  "last24h",
  "last7d",
  "last30d",
  "thisMonth",
  "lastMonth",
  "custom",
]);

function startOfLocalMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1, 0, 0, 0, 0);
}

function endOfLocalMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth() + 1, 0, 23, 59, 59, 999);
}

export function computeTimeRange(
  preset: Exclude<TimePreset, "all" | "custom">,
  now = new Date(),
): [number, number] {
  const end = now.getTime();
  switch (preset) {
    case "last24h":
      return [end - 24 * 60 * 60 * 1000, end];
    case "last7d":
      return [end - 7 * 24 * 60 * 60 * 1000, end];
    case "last30d":
      return [end - 30 * 24 * 60 * 60 * 1000, end];
    case "thisMonth":
      return [startOfLocalMonth(now).getTime(), end];
    case "lastMonth": {
      const firstDayThisMonth = startOfLocalMonth(now);
      const lastDayPrevMonth = new Date(firstDayThisMonth.getTime() - 1);
      return [
        startOfLocalMonth(lastDayPrevMonth).getTime(),
        endOfLocalMonth(lastDayPrevMonth).getTime(),
      ];
    }
  }
}

export function resolveTimeRange(
  preset: TimePreset,
  customRange: [number, number] | null,
  now = new Date(),
): [number, number] | null {
  if (preset === "all") return null;
  if (preset === "custom") return customRange;
  return computeTimeRange(preset, now);
}
