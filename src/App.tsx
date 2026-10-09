import { useCallback, useEffect, useState } from "react";
import * as api from "./api";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { ErrorBanner } from "./components/ErrorBanner";
import { JobDetail } from "./components/JobDetail";
import { JobForm } from "./components/JobForm";
import { JobList } from "./components/JobList";
import { formatAppError } from "./format";
import type { AppError, JobDetail as Detail, JobInput, JobSummary } from "./types";

type Mode = "view" | "create" | "edit";

export default function App() {
  const [jobs, setJobs] = useState<JobSummary[]>([]);
  const [filter, setFilter] = useState("");
  const [managedOnly, setManagedOnly] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [mode, setMode] = useState<Mode>("view");
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  const showError = (e: unknown) => setError(formatAppError(api.toAppError(e)));

  const reloadList = useCallback(async () => {
    try {
      setJobs(await api.listJobs());
    } catch (e) {
      showError(e);
    }
  }, []);

  const loadDetail = useCallback(async (fileName: string) => {
    try {
      setDetail(await api.getJobDetail(fileName));
    } catch (e) {
      setDetail(null);
      showError(e);
    }
  }, []);

  useEffect(() => {
    void reloadList();
  }, [reloadList]);

  const select = (fileName: string) => {
    setSelected(fileName);
    setMode("view");
    setError(null);
    void loadDetail(fileName);
  };

  const refresh = async (fileName: string | null = selected) => {
    await reloadList();
    if (fileName) await loadDetail(fileName);
  };

  /** 操作を実行し、成功したら一覧と詳細を再取得する。 */
  const act = async (fn: () => Promise<unknown>, after: string | null = selected) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
      await refresh(after);
    } catch (e) {
      showError(e);
    } finally {
      setBusy(false);
    }
  };

  const submit = async (input: JobInput): Promise<AppError | null> => {
    try {
      const fileName =
        mode === "edit" && selected ? await api.updateJob(selected, input) : await api.createJob(input);
      setMode("view");
      setSelected(fileName);
      await refresh(fileName);
      return null;
    } catch (e) {
      return api.toAppError(e);
    }
  };

  const doDelete = async () => {
    setConfirmDelete(false);
    if (!selected) return;
    const target = selected;
    setBusy(true);
    setError(null);
    try {
      await api.deleteJob(target);
      setSelected(null);
      setDetail(null);
      await reloadList();
    } catch (e) {
      showError(e);
    } finally {
      setBusy(false);
    }
  };

  // 重複判定用。編集中のジョブ自身は除く
  const existingLabels = jobs
    .filter((j) => !(mode === "edit" && j.fileName === selected))
    .flatMap((j) => [j.label ?? "", j.fileName.replace(/\.plist$/, "")])
    .filter(Boolean);

  let main;
  if (mode === "create" || (mode === "edit" && detail?.form)) {
    main = (
      <JobForm
        key={mode === "edit" ? `edit-${selected}` : "create"}
        mode={mode}
        initial={mode === "edit" ? detail!.form! : undefined}
        existingLabels={existingLabels}
        originalLabel={mode === "edit" ? detail!.form!.label : undefined}
        onSubmit={submit}
        onCancel={() => setMode("view")}
      />
    );
  } else if (detail) {
    main = (
      <JobDetail
        detail={detail}
        busy={busy}
        onToggleEnabled={(enabled) => act(() => api.setJobEnabled(detail.fileName, enabled))}
        onRunNow={() => act(() => api.runJobNow(detail.fileName))}
        onEdit={() => setMode("edit")}
        onDelete={() => setConfirmDelete(true)}
      />
    );
  } else {
    main = <p className="empty">左の一覧からジョブを選択してください</p>;
  }

  return (
    <div className="layout">
      <JobList
        jobs={jobs}
        selected={selected}
        onSelect={select}
        filter={filter}
        onFilterChange={setFilter}
        managedOnly={managedOnly}
        onManagedOnlyChange={setManagedOnly}
        onReload={() => void refresh()}
        onCreate={() => {
          setMode("create");
          setError(null);
        }}
      />
      <main className="detail-pane">
        {error && <ErrorBanner message={error} onClose={() => setError(null)} />}
        {main}
      </main>
      <ConfirmDialog
        open={confirmDelete}
        title="ジョブを削除しますか？"
        message={`${detail?.label ?? selected ?? ""} の登録を解除し、plist を削除します。ログと実行記録は残ります。`}
        confirmLabel="削除する"
        onConfirm={() => void doDelete()}
        onCancel={() => setConfirmDelete(false)}
      />
    </div>
  );
}
