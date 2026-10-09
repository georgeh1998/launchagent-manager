import type { JobInput, ScheduleInput } from "./types";

// メッセージは Rust 側（src-tauri/src/domain/validate.rs）と同じ文字列
export const MSG = {
  labelEmpty: "名前を入力してください",
  labelTooLong: "名前は200文字以内で入力してください",
  labelChars: "名前に使える文字は英数字と . _ - のみです",
  labelDot: "名前を . で始めることはできません",
  labelDup: "同じ名前のジョブがすでにあります",
  commandEmpty: "コマンドを入力してください",
  hour: "時は0〜23で指定してください",
  minute: "分は0〜59で指定してください",
  weekdays: "曜日を1つ以上選択してください",
  day: "日付は1〜31で指定してください",
  every: "間隔は1分以上で指定してください",
  dayWarning: "29〜31日を指定すると、その日がない月には実行されません",
} as const;

const inRange = (v: number, lo: number, hi: number) => Number.isInteger(v) && v >= lo && v <= hi;

export function labelFormatError(label: string): string | null {
  if (label.length === 0) return MSG.labelEmpty;
  if ([...label].length > 200) return MSG.labelTooLong;
  if (!/^[A-Za-z0-9._-]+$/.test(label)) return MSG.labelChars;
  if (label.startsWith(".")) return MSG.labelDot;
  return null;
}

function checkTime(errors: Record<string, string>, hour: number, minute: number) {
  if (!inRange(hour, 0, 23)) errors["schedule.hour"] = MSG.hour;
  if (!inRange(minute, 0, 59)) errors["schedule.minute"] = MSG.minute;
}

export function validateJobInput(
  input: JobInput,
  existingLabels: string[],
  originalLabel?: string,
): Record<string, string> {
  const errors: Record<string, string> = {};
  const labelError = labelFormatError(input.label);
  if (labelError) {
    errors.label = labelError;
  } else {
    const lower = input.label.toLowerCase();
    const isSelf = originalLabel !== undefined && originalLabel.toLowerCase() === lower;
    if (!isSelf && existingLabels.some((l) => l.toLowerCase() === lower)) errors.label = MSG.labelDup;
  }
  if (input.command.trim() === "") errors.command = MSG.commandEmpty;

  const s = input.schedule;
  switch (s.kind) {
    case "daily":
      checkTime(errors, s.hour, s.minute);
      break;
    case "weekly":
      if (s.weekdays.length === 0 || s.weekdays.some((d) => !inRange(d, 0, 7))) {
        errors["schedule.weekdays"] = MSG.weekdays;
      }
      checkTime(errors, s.hour, s.minute);
      break;
    case "monthly":
      if (!inRange(s.day, 1, 31)) errors["schedule.day"] = MSG.day;
      checkTime(errors, s.hour, s.minute);
      break;
    case "interval":
      if (!inRange(s.every, 1, 1_000_000)) errors["schedule.every"] = MSG.every;
      break;
  }
  return errors;
}

export function scheduleWarnings(s: ScheduleInput): string[] {
  return s.kind === "monthly" && s.day >= 29 && s.day <= 31 ? [MSG.dayWarning] : [];
}
