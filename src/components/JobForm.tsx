import { useState, type FormEvent } from "react";
import { formatAppError } from "../format";
import type { AppError, JobFormData, JobInput, ScheduleInput } from "../types";
import { scheduleWarnings, validateJobInput } from "../validation";

interface Props {
  mode: "create" | "edit";
  initial?: JobFormData;
  existingLabels: string[];
  originalLabel?: string;
  onSubmit(input: JobInput): Promise<AppError | null>;
  onCancel(): void;
}

type Kind = ScheduleInput["kind"];

// 表示順は月→日。値は 0=日
const WEEKDAYS: [number, string][] = [
  [1, "月"],
  [2, "火"],
  [3, "水"],
  [4, "木"],
  [5, "金"],
  [6, "土"],
  [0, "日"],
];

const toNum = (s: string) => (s.trim() === "" ? Number.NaN : Number(s));

export function JobForm({ mode, initial, existingLabels, originalLabel, onSubmit, onCancel }: Props) {
  const s = initial?.schedule ?? null;
  const [label, setLabel] = useState(initial?.label ?? "");
  const [command, setCommand] = useState(initial?.command ?? "");
  const [kind, setKind] = useState<Kind>(s?.kind ?? "daily");
  const [weekdays, setWeekdays] = useState<number[]>(s?.kind === "weekly" ? s.weekdays.map((d) => d % 7) : [1]);
  const [day, setDay] = useState(String(s?.kind === "monthly" ? s.day : 1));
  const [hour, setHour] = useState(String(s && s.kind !== "interval" ? s.hour : 9));
  const [minute, setMinute] = useState(String(s && s.kind !== "interval" ? s.minute : 0));
  const [every, setEvery] = useState(String(s?.kind === "interval" ? s.every : 30));
  const [unit, setUnit] = useState<"minutes" | "hours">(s?.kind === "interval" ? s.unit : "minutes");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const buildSchedule = (): ScheduleInput => {
    const time = { hour: toNum(hour), minute: toNum(minute) };
    switch (kind) {
      case "daily":
        return { kind, ...time };
      case "weekly":
        return { kind, weekdays: [...weekdays].sort((a, b) => a - b), ...time };
      case "monthly":
        return { kind, day: toNum(day), ...time };
      case "interval":
        return { kind, every: toNum(every), unit };
    }
  };

  const schedule = buildSchedule();
  const warnings = scheduleWarnings(schedule);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const input: JobInput = { label, command, schedule };
    const clientErrors = validateJobInput(input, existingLabels, originalLabel);
    setErrors(clientErrors);
    setFormError(null);
    if (Object.keys(clientErrors).length > 0) return;
    setSaving(true);
    const err = await onSubmit(input);
    setSaving(false);
    if (!err) return;
    if (err.kind === "validation") {
      setErrors(Object.fromEntries(err.errors.map((x) => [x.field, x.message])));
    } else {
      setFormError(formatAppError(err));
    }
  };

  const toggleWeekday = (d: number) =>
    setWeekdays((cur) => (cur.includes(d) ? cur.filter((x) => x !== d) : [...cur, d]));

  const fieldError = (key: string) =>
    errors[key] ? (
      <span className="field-error" id={`err-${key}`}>
        {errors[key]}
      </span>
    ) : null;

  const timeInputs = (
    <div className="field">
      <span className="field__label">時刻</span>
      <div className="field__row">
        <input
          className="input input--narrow"
          type="number"
          aria-label="時"
          min={0}
          max={23}
          value={hour}
          aria-invalid={!!errors["schedule.hour"]}
          onChange={(e) => setHour(e.target.value)}
        />
        <span>時</span>
        <input
          className="input input--narrow"
          type="number"
          aria-label="分"
          min={0}
          max={59}
          value={minute}
          aria-invalid={!!errors["schedule.minute"]}
          onChange={(e) => setMinute(e.target.value)}
        />
        <span>分</span>
      </div>
      {fieldError("schedule.hour")}
      {fieldError("schedule.minute")}
    </div>
  );

  return (
    <form onSubmit={handleSubmit} noValidate aria-label={mode === "create" ? "ジョブの新規作成" : "ジョブの編集"}>
      <h1 className="detail-pane__title">{mode === "create" ? "新規作成" : "編集"}</h1>
      {formError && (
        <div className="error-banner" role="alert">
          {formError}
        </div>
      )}
      <div className="field">
        <label className="field__label" htmlFor="job-label">
          名前（Label）
        </label>
        <input
          id="job-label"
          className="input mono"
          aria-label="名前"
          placeholder="com.example.backup"
          value={label}
          aria-invalid={!!errors.label}
          onChange={(e) => setLabel(e.target.value)}
        />
        {fieldError("label")}
      </div>
      <div className="field">
        <label className="field__label" htmlFor="job-command">
          実行するコマンド
        </label>
        <input
          id="job-command"
          className="input mono"
          aria-label="実行するコマンド"
          placeholder='echo "hello" >> ~/hello.txt'
          value={command}
          aria-invalid={!!errors.command}
          onChange={(e) => setCommand(e.target.value)}
        />
        {fieldError("command")}
      </div>
      <div className="field">
        <label className="field__label" htmlFor="job-kind">
          スケジュール
        </label>
        <select
          id="job-kind"
          className="select"
          aria-label="スケジュール"
          value={kind}
          onChange={(e) => setKind(e.target.value as Kind)}
        >
          <option value="daily">毎日</option>
          <option value="weekly">毎週</option>
          <option value="monthly">毎月</option>
          <option value="interval">一定間隔</option>
        </select>
        {mode === "edit" && initial && initial.schedule === null && (
          <span className="field-warning">
            既存のスケジュールはフォームで表現できないため、保存すると置き換わります
          </span>
        )}
      </div>

      {kind === "weekly" && (
        <div className="field">
          <span className="field__label">曜日</span>
          <div className="field__row">
            {WEEKDAYS.map(([d, name]) => (
              <label key={d} className="checkbox">
                <input type="checkbox" checked={weekdays.includes(d)} onChange={() => toggleWeekday(d)} />
                {name}
              </label>
            ))}
          </div>
          {fieldError("schedule.weekdays")}
        </div>
      )}

      {kind === "monthly" && (
        <div className="field">
          <span className="field__label">日付</span>
          <div className="field__row">
            <input
              className="input input--narrow"
              type="number"
              aria-label="日"
              min={1}
              max={31}
              value={day}
              aria-invalid={!!errors["schedule.day"]}
              onChange={(e) => setDay(e.target.value)}
            />
            <span>日</span>
          </div>
          {fieldError("schedule.day")}
          {warnings.map((w) => (
            <span key={w} className="field-warning">
              {w}
            </span>
          ))}
        </div>
      )}

      {kind === "interval" ? (
        <div className="field">
          <span className="field__label">間隔</span>
          <div className="field__row">
            <input
              className="input input--narrow"
              type="number"
              aria-label="間隔"
              min={1}
              value={every}
              aria-invalid={!!errors["schedule.every"]}
              onChange={(e) => setEvery(e.target.value)}
            />
            <select
              className="select"
              aria-label="単位"
              value={unit}
              onChange={(e) => setUnit(e.target.value as "minutes" | "hours")}
            >
              <option value="minutes">分ごと</option>
              <option value="hours">時間ごと</option>
            </select>
          </div>
          {fieldError("schedule.every")}
        </div>
      ) : (
        timeInputs
      )}

      <div className="detail-actions">
        <button type="submit" className="btn btn--primary" disabled={saving}>
          保存
        </button>
        <button type="button" className="btn btn--secondary" onClick={onCancel} disabled={saving}>
          キャンセル
        </button>
      </div>
    </form>
  );
}
