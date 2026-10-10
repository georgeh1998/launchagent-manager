import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { JobDetail as Detail } from "../types";
import { managedDetail, unmanagedDetail } from "../test/fixtures";
import { JobDetail } from "./JobDetail";

const renderDetail = (detail: Detail) =>
  render(
    <JobDetail detail={detail} onToggleEnabled={vi.fn()} onRunNow={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} />,
  );

describe("JobDetail", () => {
  it("shows all items for a managed job", () => {
    renderDetail(managedDetail());
    expect(screen.getByText("/Users/u/Library/LaunchAgents/com.georgeh.backup.plist")).toBeInTheDocument();
    expect(screen.getByText("毎週 月・水・金 09:00")).toBeInTheDocument();
    expect(screen.getByText("ログイン時に実行")).toBeInTheDocument();
    expect(screen.getByText('echo "a b" $HOME')).toBeInTheDocument();
    expect(screen.getByText("/Users/u/Library/Logs/com.georgeh.backup/stdout.log")).toBeInTheDocument();
    expect(screen.getByText("未設定")).toBeInTheDocument();
    expect(screen.getByText("成功")).toBeInTheDocument();
    expect(screen.getByText("開始: 2026/10/09 09:00:00")).toBeInTheDocument();
    expect(screen.getByText("終了コード: 0")).toBeInTheDocument();
    expect(screen.getByText("launchd 登録: 登録済み")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "編集" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "削除" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "無効にする" })).toBeInTheDocument();
  });

  it("hides edit/delete for unmanaged jobs and shows estimates", () => {
    renderDetail(unmanagedDetail());
    expect(screen.queryByRole("button", { name: "編集" })).toBeNull();
    expect(screen.queryByRole("button", { name: "削除" })).toBeNull();
    expect(screen.getByText("閲覧専用")).toBeInTheDocument();
    expect(screen.getByText(/推定/)).toBeInTheDocument();
    expect(screen.getByText("実行回数: 684")).toBeInTheDocument();
    expect(screen.getByText("最終終了コード: 0")).toBeInTheDocument();
    expect(screen.getByText("標準エラーログ更新日時: 不明")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "有効にする" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "今すぐ実行" })).toBeDisabled();
  });

  it("does not allow disabling an enabled unmanaged job", () => {
    const d = unmanagedDetail();
    renderDetail({ ...d, state: { ...d.state, status: "enabled", disabled: false, loaded: true } });
    expect(screen.getByRole("button", { name: "無効にする" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "今すぐ実行" })).toBeDisabled();
  });

  it("shows print and parse errors", () => {
    renderDetail(
      managedDetail({
        parseError: "Label がありません",
        state: { loaded: null, disabled: false, pid: null, status: "unknown", printError: "終了コード 1: boom" },
      }),
    );
    expect(screen.getByText(/読み込みエラー: Label がありません/)).toBeInTheDocument();
    expect(screen.getByText(/状態を取得できません: 終了コード 1: boom/)).toBeInTheDocument();
    expect(screen.getByText("launchd 登録: 不明")).toBeInTheDocument();
  });

  it("shows run record variants", () => {
    const withRecord = (record: Extract<Detail["lastRun"], { kind: "managed" }>["record"]) =>
      managedDetail({ lastRun: { kind: "managed", record } });
    const { unmount } = renderDetail(withRecord({ kind: "never" }));
    expect(screen.getByText("まだ実行されていません")).toBeInTheDocument();
    unmount();
    renderDetail(withRecord({ kind: "corrupt", message: "bad" }));
    expect(screen.getByText(/実行記録を読み取れません/)).toBeInTheDocument();
  });

  it("shows failure and running records", () => {
    const { unmount } = renderDetail(
      managedDetail({
        lastRun: {
          kind: "managed",
          record: { kind: "record", startedAt: "2026-10-09T00:00:00Z", finishedAt: "2026-10-09T00:00:01Z", exitCode: 2, outcome: "failure" },
        },
      }),
    );
    expect(screen.getByText("失敗")).toBeInTheDocument();
    expect(screen.getByText("終了コード: 2")).toBeInTheDocument();
    unmount();
    renderDetail(
      managedDetail({
        state: { loaded: true, disabled: false, pid: 123, status: "enabled", printError: null },
        lastRun: {
          kind: "managed",
          record: { kind: "record", startedAt: "2026-10-09T00:00:00Z", finishedAt: null, exitCode: null, outcome: "running" },
        },
      }),
    );
    expect(screen.getByText("実行中（または中断）")).toBeInTheDocument();
    expect(screen.getByText("PID: 123")).toBeInTheDocument();
  });
});
