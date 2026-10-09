import { describe, expect, it } from "vitest";
import { MSG, scheduleWarnings, validateJobInput } from "./validation";
import type { JobInput, ScheduleInput } from "./types";

const daily: ScheduleInput = { kind: "daily", hour: 9, minute: 0 };
const input = (label: string, command = "echo", schedule: ScheduleInput = daily): JobInput => ({
  label,
  command,
  schedule,
});

describe("validateJobInput", () => {
  it("rejects bad labels", () => {
    for (const bad of ["", "a b", "a/b", "../evil", ".hidden", "..", "日本語", "a".repeat(201)]) {
      expect(validateJobInput(input(bad), []).label, bad).toBeDefined();
    }
    expect(validateJobInput(input("a".repeat(201)), []).label).toBe(MSG.labelTooLong);
    expect(validateJobInput(input(".x"), []).label).toBe(MSG.labelDot);
    expect(validateJobInput(input("com.example.job_1-2"), [])).toEqual({});
  });

  it("detects duplicates case-insensitively but allows self", () => {
    expect(validateJobInput(input("COM.X"), ["com.x"]).label).toBe(MSG.labelDup);
    expect(validateJobInput(input("com.x"), ["com.x"], "com.x")).toEqual({});
  });

  it("requires command", () => {
    expect(validateJobInput(input("a", "  "), []).command).toBe(MSG.commandEmpty);
  });

  it("checks ranges", () => {
    const v = (s: ScheduleInput) => Object.keys(validateJobInput(input("a", "x", s), []));
    expect(v({ kind: "daily", hour: -1, minute: 0 })).toEqual(["schedule.hour"]);
    expect(v({ kind: "daily", hour: 24, minute: 0 })).toEqual(["schedule.hour"]);
    expect(v({ kind: "daily", hour: 0, minute: 60 })).toEqual(["schedule.minute"]);
    expect(v({ kind: "weekly", weekdays: [], hour: 0, minute: 0 })).toEqual(["schedule.weekdays"]);
    expect(v({ kind: "monthly", day: 0, hour: 0, minute: 0 })).toEqual(["schedule.day"]);
    expect(v({ kind: "monthly", day: 32, hour: 0, minute: 0 })).toEqual(["schedule.day"]);
    expect(v({ kind: "interval", every: 0, unit: "minutes" })).toEqual(["schedule.every"]);
    expect(v({ kind: "daily", hour: Number.NaN, minute: 0 })).toEqual(["schedule.hour"]);
  });

  it("accepts boundaries", () => {
    const ok: ScheduleInput[] = [
      { kind: "daily", hour: 0, minute: 0 },
      { kind: "daily", hour: 23, minute: 59 },
      { kind: "monthly", day: 31, hour: 0, minute: 0 },
      { kind: "interval", every: 1, unit: "minutes" },
    ];
    for (const s of ok) expect(validateJobInput(input("a", "x", s), [])).toEqual({});
  });
});

describe("scheduleWarnings", () => {
  it("warns for day >= 29", () => {
    expect(scheduleWarnings({ kind: "monthly", day: 29, hour: 0, minute: 0 })).toEqual([MSG.dayWarning]);
    expect(scheduleWarnings({ kind: "monthly", day: 28, hour: 0, minute: 0 })).toEqual([]);
  });
});
