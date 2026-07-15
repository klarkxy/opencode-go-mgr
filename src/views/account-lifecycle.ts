const DAY_MS = 24 * 60 * 60 * 1000;
const DATE_PATTERN = /^(\d{4})-(\d{2})-(\d{2})$/;

export type ExpiryTagType = "success" | "warning" | "error";

function localDayNumber(value: Date): number {
  return Date.UTC(value.getFullYear(), value.getMonth(), value.getDate()) / DAY_MS;
}

function dateStringDayNumber(value: string): number {
  const match = DATE_PATTERN.exec(value);
  if (!match) return Number.NaN;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const timestamp = Date.UTC(year, month - 1, day);
  const parsed = new Date(timestamp);
  if (
    parsed.getUTCFullYear() !== year
    || parsed.getUTCMonth() !== month - 1
    || parsed.getUTCDate() !== day
  ) return Number.NaN;
  return timestamp / DAY_MS;
}

export function localDateString(value: Date | number = Date.now()): string {
  const date = typeof value === "number" ? new Date(value) : value;
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function daysUntilDate(target: string, from: Date | number = Date.now()): number {
  const targetDay = dateStringDayNumber(target);
  if (!Number.isFinite(targetDay)) return Number.NaN;
  const source = typeof from === "number" ? new Date(from) : from;
  return targetDay - localDayNumber(source);
}

export function moveItem<T>(items: readonly T[], fromIndex: number, toIndex: number): T[] {
  const next = [...items];
  if (
    fromIndex < 0
    || toIndex < 0
    || fromIndex >= next.length
    || toIndex >= next.length
    || fromIndex === toIndex
  ) return next;
  const [item] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, item);
  return next;
}

export function expiryTagType(days: number): ExpiryTagType {
  if (!Number.isFinite(days) || days <= 0) return "error";
  return days <= 7 ? "warning" : "success";
}
