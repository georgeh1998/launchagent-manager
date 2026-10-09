import type { JobSummary } from "../types";
import { StatusLabel } from "./StatusLabel";

export function filterJobs(jobs: JobSummary[], filter: string, managedOnly: boolean): JobSummary[] {
  const q = filter.trim().toLowerCase();
  return jobs.filter(
    (j) =>
      (!managedOnly || j.managed) &&
      (q === "" || (j.label ?? "").toLowerCase().includes(q) || j.fileName.toLowerCase().includes(q)),
  );
}

interface Props {
  jobs: JobSummary[];
  selected: string | null;
  onSelect(fileName: string): void;
  filter: string;
  onFilterChange(v: string): void;
  managedOnly: boolean;
  onManagedOnlyChange(v: boolean): void;
  onReload(): void;
  onCreate(): void;
}

export function JobList(props: Props) {
  const visible = filterJobs(props.jobs, props.filter, props.managedOnly);
  return (
    <nav className="list-pane" aria-label="ジョブ一覧">
      <div className="list-pane__header">
        <input
          className="input"
          type="search"
          aria-label="絞り込み"
          placeholder="例: com.georgeh."
          value={props.filter}
          onChange={(e) => props.onFilterChange(e.target.value)}
        />
        <label className="checkbox">
          <input
            type="checkbox"
            checked={props.managedOnly}
            onChange={(e) => props.onManagedOnlyChange(e.target.checked)}
          />
          本アプリ管理のみ
        </label>
        <div className="list-pane__actions">
          <button type="button" className="btn btn--secondary" onClick={props.onReload}>
            再読み込み
          </button>
          <button type="button" className="btn btn--primary" onClick={props.onCreate}>
            新規作成
          </button>
        </div>
      </div>
      <ul className="job-list">
        {visible.map((j) => (
          <li key={j.fileName}>
            <button
              type="button"
              className="job-row"
              aria-pressed={props.selected === j.fileName}
              onClick={() => props.onSelect(j.fileName)}
            >
              <span className="job-row__name">{j.displayName}</span>
              <span className="job-row__meta">
                {j.parseError && <span className="status-label status-label--error">読み込みエラー</span>}
                {j.running && <span className="status-label status-label--enabled">実行中</span>}
                <StatusLabel status={j.status} />
              </span>
            </button>
          </li>
        ))}
        {visible.length === 0 && <li className="muted">該当するジョブはありません</li>}
      </ul>
    </nav>
  );
}
