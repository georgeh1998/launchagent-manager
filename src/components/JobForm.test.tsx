import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AppError, JobFormData } from "../types";
import { MSG } from "../validation";
import { JobForm } from "./JobForm";

function renderForm(opts: { initial?: JobFormData; existing?: string[]; result?: AppError | null; mode?: "create" | "edit" } = {}) {
  const onSubmit = vi.fn().mockResolvedValue(opts.result ?? null);
  const onCancel = vi.fn();
  render(
    <JobForm
      mode={opts.mode ?? (opts.initial ? "edit" : "create")}
      initial={opts.initial}
      existingLabels={opts.existing ?? []}
      originalLabel={opts.initial?.label}
      onSubmit={onSubmit}
      onCancel={onCancel}
    />,
  );
  return { onSubmit, onCancel };
}

const save = () => userEvent.click(screen.getByRole("button", { name: "保存" }));

async function fillBasics(label = "com.example.a", command = "echo hi") {
  await userEvent.type(screen.getByLabelText("名前"), label);
  await userEvent.type(screen.getByLabelText("実行するコマンド"), command);
}

async function setNumber(name: string, value: string) {
  const el = screen.getByLabelText(name);
  await userEvent.clear(el);
  if (value !== "") await userEvent.type(el, value);
}

describe("JobForm", () => {
  it("shows required errors and does not submit", async () => {
    const { onSubmit } = renderForm();
    await save();
    expect(screen.getByText(MSG.labelEmpty)).toBeInTheDocument();
    expect(screen.getByText(MSG.commandEmpty)).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("rejects bad characters and duplicates", async () => {
    const { onSubmit } = renderForm({ existing: ["com.taken"] });
    await fillBasics("bad label");
    await save();
    expect(screen.getByText(MSG.labelChars)).toBeInTheDocument();
    await userEvent.clear(screen.getByLabelText("名前"));
    await userEvent.type(screen.getByLabelText("名前"), "COM.TAKEN");
    await save();
    expect(screen.getByText(MSG.labelDup)).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("validates time ranges", async () => {
    const { onSubmit } = renderForm();
    await fillBasics();
    await setNumber("時", "24");
    await setNumber("分", "60");
    await save();
    expect(screen.getByText(MSG.hour)).toBeInTheDocument();
    expect(screen.getByText(MSG.minute)).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("requires a weekday", async () => {
    renderForm();
    await fillBasics();
    await userEvent.selectOptions(screen.getByLabelText("スケジュール"), "weekly");
    await userEvent.click(screen.getByRole("checkbox", { name: "月" }));
    await save();
    expect(screen.getByText(MSG.weekdays)).toBeInTheDocument();
  });

  it("validates day and warns for 29-31", async () => {
    renderForm();
    await fillBasics();
    await userEvent.selectOptions(screen.getByLabelText("スケジュール"), "monthly");
    await setNumber("日", "30");
    expect(screen.getByText(MSG.dayWarning)).toBeInTheDocument();
    await setNumber("日", "32");
    await save();
    expect(screen.getByText(MSG.day)).toBeInTheDocument();
  });

  it("validates interval", async () => {
    renderForm();
    await fillBasics();
    await userEvent.selectOptions(screen.getByLabelText("スケジュール"), "interval");
    await setNumber("間隔", "0");
    await save();
    expect(screen.getByText(MSG.every)).toBeInTheDocument();
  });

  it("submits weekly input", async () => {
    const { onSubmit } = renderForm();
    await fillBasics("com.example.a", 'echo "a b"');
    await userEvent.selectOptions(screen.getByLabelText("スケジュール"), "weekly");
    await userEvent.click(screen.getByRole("checkbox", { name: "水" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "金" }));
    await save();
    expect(onSubmit).toHaveBeenCalledWith({
      label: "com.example.a",
      command: 'echo "a b"',
      schedule: { kind: "weekly", weekdays: [1, 3, 5], hour: 9, minute: 0 },
    });
  });

  it("submits interval input", async () => {
    const { onSubmit } = renderForm();
    await fillBasics();
    await userEvent.selectOptions(screen.getByLabelText("スケジュール"), "interval");
    await setNumber("間隔", "2");
    await userEvent.selectOptions(screen.getByLabelText("単位"), "hours");
    await save();
    expect(onSubmit).toHaveBeenCalledWith({
      label: "com.example.a",
      command: "echo hi",
      schedule: { kind: "interval", every: 2, unit: "hours" },
    });
  });

  it("prefills in edit mode and allows its own label", async () => {
    const { onSubmit } = renderForm({
      initial: { label: "com.x", command: "echo x", schedule: { kind: "monthly", day: 15, hour: 7, minute: 30 } },
      existing: [],
    });
    expect(screen.getByLabelText("名前")).toHaveValue("com.x");
    expect(screen.getByLabelText("実行するコマンド")).toHaveValue("echo x");
    expect(screen.getByLabelText("日")).toHaveValue(15);
    expect(screen.getByLabelText("時")).toHaveValue(7);
    await save();
    expect(onSubmit).toHaveBeenCalledWith({
      label: "com.x",
      command: "echo x",
      schedule: { kind: "monthly", day: 15, hour: 7, minute: 30 },
    });
  });

  it("warns when existing schedule cannot be represented", () => {
    renderForm({ initial: { label: "com.x", command: "x", schedule: null } });
    expect(screen.getByText(/保存すると置き換わります/)).toBeInTheDocument();
  });

  it("shows server validation errors on fields", async () => {
    renderForm({ result: { kind: "validation", errors: [{ field: "label", message: MSG.labelDup }] } });
    await fillBasics();
    await save();
    expect(await screen.findByText(MSG.labelDup)).toBeInTheDocument();
  });

  it("shows other server errors at the top", async () => {
    renderForm({ result: { kind: "launchctl", operation: "bootstrap", exitCode: 5, stderr: "boom", rolledBack: true } });
    await fillBasics();
    await save();
    expect(await screen.findByRole("alert")).toHaveTextContent("終了コード 5");
  });
});
