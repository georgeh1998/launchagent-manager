import type { AppError, JobStatus } from "./types";

const pad = (n: number) => String(n).padStart(2, "0");

/** ISO 文字列または epoch 秒をローカル時刻 `YYYY/MM/DD HH:MM:SS` にする。 */
export function formatDateTime(value: string | number | null): string {
  if (value === null) return "-";
  const d = typeof value === "number" ? new Date(value * 1000) : new Date(value);
  if (Number.isNaN(d.getTime())) return String(value);
  return (
    `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

export function formatAppError(e: AppError): string {
  switch (e.kind) {
    case "validation":
      return e.errors.map((x) => x.message).join(" / ");
    case "notManaged":
      return "本アプリで作成したジョブ以外は編集・削除できません";
    case "notFound":
      return `ジョブが見つかりません: ${e.fileName}`;
    case "invalidPath":
      return `不正なパスです: ${e.path}`;
    case "launchctl": {
      const base = `launchctl ${e.operation} が失敗しました（終了コード ${e.exitCode}）: ${e.stderr}`;
      return e.rolledBack ? `${base}（変更前の状態に戻しました）` : base;
    }
    case "io":
    case "plist":
      return e.message;
  }
}

export function statusLabel(status: JobStatus): string {
  return status === "enabled" ? "有効" : status === "disabled" ? "無効" : "不明";
}
