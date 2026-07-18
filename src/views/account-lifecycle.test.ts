import assert from "node:assert/strict";
import test from "node:test";
import {
  daysUntilDate,
  expiryTagType,
  localDateString,
  moveItem,
  purchaseExpiresOn,
} from "./account-lifecycle.ts";

test("formats dates and compares calendar days without time-of-day drift", () => {
  const late = new Date(2026, 6, 15, 23, 59, 59);
  assert.equal(localDateString(late), "2026-07-15");
  assert.equal(daysUntilDate("2026-07-16", late), 1);
  assert.equal(daysUntilDate("2026-07-15", late), 0);
  assert.equal(daysUntilDate("2026-07-14", late), -1);
});

test("date differences stay correct across leap day and reject invalid dates", () => {
  assert.equal(daysUntilDate("2026-02-28", new Date(2026, 0, 31)), 28);
  assert.equal(daysUntilDate("2024-03-01", new Date(2024, 1, 28)), 2);
  assert.equal(daysUntilDate("2024-02-29", new Date(2024, 1, 28)), 1);
  assert.equal(Number.isNaN(daysUntilDate("2026-02-30", new Date(2026, 1, 1))), true);
});

test("moves items in either direction without mutating the source", () => {
  const source = ["a", "b", "c", "d"];
  assert.deepEqual(moveItem(source, 0, 2), ["b", "c", "a", "d"]);
  assert.deepEqual(moveItem(source, 3, 1), ["a", "d", "b", "c"]);
  assert.deepEqual(moveItem(source, 0, 3), ["b", "c", "d", "a"]);
  assert.deepEqual(moveItem(source, 3, 0), ["d", "a", "b", "c"]);
  assert.deepEqual(moveItem(source, -1, 2), source);
  assert.deepEqual(source, ["a", "b", "c", "d"]);
});

test("computes next-month expiry from purchase date, clamped to month end", () => {
  assert.equal(purchaseExpiresOn("2026-01-15"), "2026-02-15");
  assert.equal(purchaseExpiresOn("2026-01-31"), "2026-02-28");
  assert.equal(purchaseExpiresOn("2024-01-31"), "2024-02-29");
  assert.equal(purchaseExpiresOn("2024-12-15"), "2025-01-15");
  assert.equal(purchaseExpiresOn("not-a-date"), null);
});

test("uses warning for imminent expiry and error after expiry", () => {
  assert.equal(expiryTagType(8), "success");
  assert.equal(expiryTagType(7), "warning");
  assert.equal(expiryTagType(1), "warning");
  assert.equal(expiryTagType(0), "error");
  assert.equal(expiryTagType(-1), "error");
  assert.equal(expiryTagType(Number.NaN), "error");
});
