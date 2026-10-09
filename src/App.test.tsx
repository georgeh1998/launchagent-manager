import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { JOBS, managedDetail, unmanagedDetail } from "./test/fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke);

type Handler = (args?: Record<string, unknown>) => unknown;
let handlers: Record<string, Handler>;

beforeEach(() => {
  mockInvoke.mockReset();
  handlers = {
    list_jobs: () => JOBS,
    get_job_detail: (a) => (a?.fileName === "com.georgeh.backup.plist" ? managedDetail() : unmanagedDetail()),
    create_job: () => "com.example.new.plist",
    update_job: () => "com.georgeh.backup.plist",
    delete_job: () => null,
    set_job_enabled: () => null,
    run_job_now: () => null,
  };
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    const h = handlers[cmd];
    if (!h) throw new Error(`unexpected ${cmd}`);
    return h(args as Record<string, unknown>);
  });
});

const calls = (cmd: string) => mockInvoke.mock.calls.filter(([c]) => c === cmd);

async function selectRow(name: RegExp) {
  await userEvent.click(await screen.findByRole("button", { name }));
}

describe("App", () => {
  it("lists all jobs with an empty filter initially", async () => {
    render(<App />);
    expect(await screen.findByRole("button", { name: /com\.georgeh\.backup/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /atlas/ })).toBeInTheDocument();
    expect(screen.getByLabelText("絞り込み")).toHaveValue("");
    expect(calls("list_jobs")).toHaveLength(1);
  });

  it("filters by text", async () => {
    render(<App />);
    await screen.findByRole("button", { name: /atlas/ });
    await userEvent.type(screen.getByLabelText("絞り込み"), "com.georgeh.");
    expect(screen.queryByRole("button", { name: /atlas/ })).toBeNull();
    expect(screen.getByRole("button", { name: /com\.georgeh\.backup/ })).toBeInTheDocument();
  });

  it("switches detail on selection", async () => {
    render(<App />);
    await selectRow(/com\.georgeh\.backup/);
    expect(await screen.findByText("毎週 月・水・金 09:00")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_job_detail", { fileName: "com.georgeh.backup.plist" });
    await selectRow(/atlas/);
    expect(await screen.findByText("1時間ごと")).toBeInTheDocument();
    expect(screen.queryByText("毎週 月・水・金 09:00")).toBeNull();
  });

  it("reloads the list", async () => {
    render(<App />);
    await screen.findByRole("button", { name: /atlas/ });
    await userEvent.click(screen.getByRole("button", { name: "再読み込み" }));
    await waitFor(() => expect(calls("list_jobs")).toHaveLength(2));
  });

  it("creates a job", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "新規作成" }));
    await userEvent.type(screen.getByLabelText("名前"), "com.example.new");
    await userEvent.type(screen.getByLabelText("実行するコマンド"), "echo hi");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("create_job", {
        input: { label: "com.example.new", command: "echo hi", schedule: { kind: "daily", hour: 9, minute: 0 } },
      }),
    );
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_job_detail", { fileName: "com.example.new.plist" }));
  });

  it("edits a job", async () => {
    render(<App />);
    await selectRow(/com\.georgeh\.backup/);
    await userEvent.click(await screen.findByRole("button", { name: "編集" }));
    const cmd = screen.getByLabelText("実行するコマンド");
    await userEvent.clear(cmd);
    await userEvent.type(cmd, "echo new");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("update_job", {
        fileName: "com.georgeh.backup.plist",
        input: {
          label: "com.georgeh.backup",
          command: "echo new",
          schedule: { kind: "weekly", weekdays: [1, 3, 5], hour: 9, minute: 0 },
        },
      }),
    );
  });

  it("does not delete when the dialog is cancelled", async () => {
    render(<App />);
    await selectRow(/com\.georgeh\.backup/);
    await userEvent.click(await screen.findByRole("button", { name: "削除" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "キャンセル" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("delete_job")).toHaveLength(0);
  });

  it("deletes after confirmation", async () => {
    render(<App />);
    await selectRow(/com\.georgeh\.backup/);
    await userEvent.click(await screen.findByRole("button", { name: "削除" }));
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "削除する" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("delete_job", { fileName: "com.georgeh.backup.plist" }));
  });

  it("toggles enabled and runs now", async () => {
    render(<App />);
    await selectRow(/com\.georgeh\.backup/);
    await userEvent.click(await screen.findByRole("button", { name: "無効にする" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("set_job_enabled", { fileName: "com.georgeh.backup.plist", enabled: false }),
    );
    await userEvent.click(screen.getByRole("button", { name: "今すぐ実行" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("run_job_now", { fileName: "com.georgeh.backup.plist" }));
  });

  it("hides edit/delete for unmanaged jobs", async () => {
    render(<App />);
    await selectRow(/atlas/);
    await screen.findByText("1時間ごと");
    expect(screen.queryByRole("button", { name: "編集" })).toBeNull();
    expect(screen.queryByRole("button", { name: "削除" })).toBeNull();
  });

  it("shows launchctl errors with exit code and stderr", async () => {
    handlers.run_job_now = () => {
      throw { kind: "launchctl", operation: "kickstart", exitCode: 113, stderr: "Could not find service", rolledBack: false };
    };
    render(<App />);
    await selectRow(/com\.georgeh\.backup/);
    await userEvent.click(await screen.findByRole("button", { name: "今すぐ実行" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("終了コード 113");
    expect(alert).toHaveTextContent("Could not find service");
  });
});
