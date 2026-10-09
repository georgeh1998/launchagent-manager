import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { JOBS } from "../test/fixtures";
import { filterJobs, JobList } from "./JobList";

describe("filterJobs", () => {
  it("returns all for empty filter", () => {
    expect(filterJobs(JOBS, "", false)).toHaveLength(3);
  });
  it("filters by partial match, case-insensitive", () => {
    expect(filterJobs(JOBS, "com.georgeh.", false).map((j) => j.fileName)).toEqual(["com.georgeh.backup.plist"]);
    expect(filterJobs(JOBS, "ATLAS", false)).toHaveLength(1);
    expect(filterJobs(JOBS, "broken", false)).toHaveLength(1);
  });
  it("filters managed only", () => {
    expect(filterJobs(JOBS, "", true).map((j) => j.fileName)).toEqual(["com.georgeh.backup.plist"]);
  });
});

function renderList(over: Partial<Parameters<typeof JobList>[0]> = {}) {
  const props = {
    jobs: JOBS,
    selected: null,
    onSelect: vi.fn(),
    filter: "",
    onFilterChange: vi.fn(),
    managedOnly: false,
    onManagedOnlyChange: vi.fn(),
    onReload: vi.fn(),
    onCreate: vi.fn(),
    ...over,
  };
  render(<JobList {...props} />);
  return props;
}

describe("JobList", () => {
  it("shows rows with status labels and parse errors", () => {
    renderList();
    const backup = screen.getByRole("button", { name: /com\.georgeh\.backup/ });
    expect(within(backup).getByText("有効")).toBeInTheDocument();
    const atlas = screen.getByRole("button", { name: /atlas/ });
    expect(within(atlas).getByText("無効")).toBeInTheDocument();
    const broken = screen.getByRole("button", { name: /broken\.plist/ });
    expect(within(broken).getByText("読み込みエラー")).toBeInTheDocument();
  });

  it("calls onSelect when a row is clicked", async () => {
    const props = renderList();
    await userEvent.click(screen.getByRole("button", { name: /atlas/ }));
    expect(props.onSelect).toHaveBeenCalledWith("com.openai.atlas.update-helper.plist");
  });

  it("applies filter prop", () => {
    renderList({ filter: "georgeh" });
    expect(screen.queryByRole("button", { name: /atlas/ })).toBeNull();
  });

  it("wires reload, create and managed-only", async () => {
    const props = renderList();
    await userEvent.click(screen.getByRole("button", { name: "再読み込み" }));
    await userEvent.click(screen.getByRole("button", { name: "新規作成" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "本アプリ管理のみ" }));
    expect(props.onReload).toHaveBeenCalled();
    expect(props.onCreate).toHaveBeenCalled();
    expect(props.onManagedOnlyChange).toHaveBeenCalledWith(true);
  });
});
