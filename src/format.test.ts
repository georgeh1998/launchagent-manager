import { describe, expect, it } from "vitest";
import { formatAppError, formatDateTime, statusLabel } from "./format";

describe("formatDateTime", () => {
  it("formats ISO in local time (Asia/Tokyo)", () => {
    expect(formatDateTime("2026-10-09T00:00:05Z")).toBe("2026/10/09 09:00:05");
  });
  it("formats epoch seconds", () => {
    expect(formatDateTime(0)).toBe("1970/01/01 09:00:00");
  });
  it("handles null", () => {
    expect(formatDateTime(null)).toBe("-");
  });
});

describe("formatAppError", () => {
  it("formats launchctl errors", () => {
    expect(
      formatAppError({ kind: "launchctl", operation: "bootstrap", exitCode: 5, stderr: "boom", rolledBack: false }),
    ).toBe("launchctl bootstrap が失敗しました（終了コード 5）: boom");
    expect(
      formatAppError({ kind: "launchctl", operation: "bootstrap", exitCode: 5, stderr: "boom", rolledBack: true }),
    ).toContain("変更前の状態に戻しました");
  });
  it("formats other kinds", () => {
    expect(formatAppError({ kind: "notManaged", fileName: "a" })).toBe(
      "本アプリで作成したジョブ以外は編集・削除できません",
    );
    expect(formatAppError({ kind: "io", message: "x" })).toBe("x");
    expect(formatAppError({ kind: "validation", errors: [{ field: "label", message: "m1" }] })).toBe("m1");
    expect(formatAppError({ kind: "notFound", fileName: "a.plist" })).toContain("a.plist");
  });
});

describe("statusLabel", () => {
  it("maps statuses", () => {
    expect(statusLabel("enabled")).toBe("有効");
    expect(statusLabel("disabled")).toBe("無効");
    expect(statusLabel("unknown")).toBe("不明");
  });
});
