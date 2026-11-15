// Pure time helpers of the frontend (src/lib/time.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addHoursLocal,
  formatHm,
  formatHms,
  formatMinutes,
  liveSeconds,
  localDateKey,
  parseHm,
  toLocalInput,
} from "../../src/lib/time.ts";

test("formatHms pads minutes and seconds, never wraps hours, floors negatives", () => {
  assert.equal(formatHms(0), "0:00:00");
  assert.equal(formatHms(5025), "1:23:45");
  assert.equal(formatHms(100 * 3600 + 1), "100:00:01");
  assert.equal(formatHms(59.9), "0:00:59");
  assert.equal(formatHms(-5), "0:00:00");
});

test("formatHm drops seconds", () => {
  assert.equal(formatHm(0), "0:00");
  assert.equal(formatHm(3600 + 23 * 60 + 59), "1:23");
  assert.equal(formatHm(-1), "0:00");
});

test("liveSeconds adds whole seconds since the fetch only while running", () => {
  assert.equal(liveSeconds(100, true, 1_000, 3_999), 102);
  assert.equal(liveSeconds(100, false, 1_000, 9_999), 100);
  // A clock that went backwards never subtracts.
  assert.equal(liveSeconds(100, true, 5_000, 1_000), 100);
});

test("localDateKey and toLocalInput use local fields with padding", () => {
  const d = new Date(2026, 0, 5, 9, 7, 30);
  assert.equal(localDateKey(d), "2026-01-05");
  assert.equal(toLocalInput(d), "2026-01-05T09:07");
});

test("addHoursLocal adds real hours and rejects other formats", () => {
  assert.equal(addHoursLocal("2026-10-06T09:30", 12), "2026-10-06T21:30");
  assert.equal(addHoursLocal("2026-10-06T20:00", 12), "2026-10-07T08:00");
  assert.equal(addHoursLocal("2026-10-06 09:30", 12), null);
  assert.equal(addHoursLocal("2026-10-6T9:30", 12), null);
});

test("formatMinutes matches the backend's format_hm", () => {
  assert.equal(formatMinutes(0), "0m");
  assert.equal(formatMinutes(45), "45m");
  assert.equal(formatMinutes(60), "1h 0m");
  assert.equal(formatMinutes(90), "1h 30m");
  assert.equal(formatMinutes(-3), "0m");
});

test("parseHm reads 24-hour times and normalises them to HH:MM", () => {
  assert.equal(parseHm("09:30"), "09:30");
  assert.equal(parseHm("9:30"), "09:30");
  assert.equal(parseHm(" 17:05 "), "17:05");
  assert.equal(parseHm("930"), "09:30");
  assert.equal(parseHm("2359"), "23:59");
  assert.equal(parseHm("0:00"), "00:00");
  // Full-width digits and colon from a Chinese IME.
  assert.equal(parseHm("１４：１５"), "14:15");
});

test("parseHm rejects out-of-range and malformed times", () => {
  for (const bad of ["", "24:00", "12:60", "9:3", "12", "123456", "9.30", "下午3:00", "3:00 PM", "-1:00"]) {
    assert.equal(parseHm(bad), null, bad);
  }
});
