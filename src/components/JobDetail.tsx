import { formatDateTime } from "../format";
import type { JobDetail as Detail, LastRun, RunRecordView } from "../types";
import { StatusLabel } from "./StatusLabel";

interface Props {
  detail: Detail;
  busy?: boolean;
  onToggleEnabled(enabled: boolean): void;
  onRunNow(): void;
  onEdit(): void;
  onDelete(): void;
}

const yesNo = (v: boolean | null, yes: string, no: string) => (v === null ? "不明" : v ? yes : no);

function RecordView({ record }: { record: RunRecordView }) {
  switch (record.kind) {
    case "never":
      return <span>まだ実行されていません</span>;
    case "corrupt":
      return <span className="field-error">実行記録を読み取れません（{record.message}）</span>;
    case "record": {
      const label = { success: "成功", failure: "失敗", running: "実行中（または中断）" }[record.outcome];
      const cls = { success: "enabled", failure: "error", running: "warning" }[record.outcome];
      return (
        <div>
          <span className={`status-label status-label--${cls}`}>{label}</span>
          <div>開始: {formatDateTime(record.startedAt)}</div>
          <div>終了: {formatDateTime(record.finishedAt)}</div>
          <div>終了コード: {record.exitCode ?? "-"}</div>
        </div>
      );
    }
  }
}

function LastRunView({ lastRun }: { lastRun: LastRun }) {
  if (lastRun.kind === "managed") return <RecordView record={lastRun.record} />;
  const mtime = (v: number | null) => (v === null ? "不明" : formatDateTime(v));
  return (
    <div>
      <div className="muted">（推定）launchctl とログファイルから推定した値です</div>
      <div>実行回数: {lastRun.runs ?? "不明"}</div>
      <div>最終終了コード: {lastRun.lastExitCode ?? "不明"}</div>
      <div>標準出力ログ更新日時: {mtime(lastRun.stdoutMtime)}</div>
      <div>標準エラーログ更新日時: {mtime(lastRun.stderrMtime)}</div>
    </div>
  );
}

export function JobDetail({ detail, busy, onToggleEnabled, onRunNow, onEdit, onDelete }: Props) {
  const { state } = detail;
  const enabled = state.status === "enabled";
  const hasLabel = detail.label !== null;
  // 管理外のジョブは有効/無効の切り替えも今すぐ実行もさせない
  const canOperate = detail.managed;
  const managedOnlyHint = canOperate ? undefined : "本アプリで管理していないジョブでは使えません";
  return (
    <section aria-label="ジョブの詳細">
      <h1 className="detail-pane__title">
        {detail.label ?? detail.fileName}
        <StatusLabel status={state.status} />
      </h1>
      {detail.parseError && (
        <p className="field-error" role="status">
          読み込みエラー: {detail.parseError}
        </p>
      )}
      <div className="detail-actions">
        {hasLabel && (
          <>
            <button
              type="button"
              className="btn btn--secondary"
              disabled={busy || !canOperate}
              title={managedOnlyHint}
              onClick={() => onToggleEnabled(!enabled)}
            >
              {enabled ? "無効にする" : "有効にする"}
            </button>
            <button
              type="button"
              className="btn btn--secondary"
              disabled={busy || !canOperate}
              title={managedOnlyHint}
              onClick={onRunNow}
            >
              今すぐ実行
            </button>
          </>
        )}
        {detail.managed ? (
          <>
            <button type="button" className="btn btn--primary" disabled={busy} onClick={onEdit}>
              編集
            </button>
            <button type="button" className="btn btn--danger" disabled={busy} onClick={onDelete}>
              削除
            </button>
          </>
        ) : (
          <span className="status-label status-label--disabled">閲覧専用</span>
        )}
      </div>
      <dl className="detail-list">
        <dt>Label</dt>
        <dd className="mono">{detail.label ?? "（なし）"}</dd>
        <dt>ファイル</dt>
        <dd className="mono">{detail.path}</dd>
        <dt>状態</dt>
        <dd>
          <div>launchd 登録: {yesNo(state.loaded, "登録済み", "未登録")}</div>
          <div>無効化設定: {state.disabled ? "あり（disabled）" : "なし"}</div>
          <div>PID: {state.pid ?? "実行中ではありません"}</div>
          {state.printError && <div className="field-error">状態を取得できません: {state.printError}</div>}
        </dd>
        <dt>スケジュール</dt>
        <dd>
          {detail.scheduleLines.map((l, i) => (
            <div key={i}>{l}</div>
          ))}
        </dd>
        <dt>実行コマンド</dt>
        <dd className="mono">{detail.command ?? "（なし）"}</dd>
        <dt>標準出力</dt>
        <dd className="mono">{detail.stdoutPath ?? "未設定"}</dd>
        <dt>標準エラー</dt>
        <dd className="mono">{detail.stderrPath ?? "未設定"}</dd>
        <dt>最終実行</dt>
        <dd>
          <LastRunView lastRun={detail.lastRun} />
        </dd>
      </dl>
    </section>
  );
}
