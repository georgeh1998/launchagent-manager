import type { JobDetail, JobSummary } from "../types";

export const summary = (over: Partial<JobSummary> = {}): JobSummary => ({
  fileName: "com.georgeh.backup.plist",
  path: "/Users/u/Library/LaunchAgents/com.georgeh.backup.plist",
  label: "com.georgeh.backup",
  displayName: "com.georgeh.backup",
  managed: true,
  status: "enabled",
  running: false,
  parseError: null,
  ...over,
});

export const JOBS: JobSummary[] = [
  summary(),
  summary({
    fileName: "com.openai.atlas.update-helper.plist",
    label: "com.openai.atlas.update-helper",
    displayName: "com.openai.atlas.update-helper",
    managed: false,
    status: "disabled",
  }),
  summary({
    fileName: "broken.plist",
    label: null,
    displayName: "broken.plist",
    managed: false,
    status: "unknown",
    parseError: "invalid XML",
  }),
];

export const managedDetail = (over: Partial<JobDetail> = {}): JobDetail => ({
  fileName: "com.georgeh.backup.plist",
  path: "/Users/u/Library/LaunchAgents/com.georgeh.backup.plist",
  label: "com.georgeh.backup",
  managed: true,
  parseError: null,
  state: { loaded: true, disabled: false, pid: null, status: "enabled", printError: null },
  scheduleLines: ["毎週 月・水・金 09:00", "ログイン時に実行"],
  command: 'echo "a b" $HOME',
  stdoutPath: "/Users/u/Library/Logs/com.georgeh.backup/stdout.log",
  stderrPath: null,
  lastRun: {
    kind: "managed",
    record: {
      kind: "record",
      startedAt: "2026-10-09T00:00:00Z",
      finishedAt: "2026-10-09T00:00:05Z",
      exitCode: 0,
      outcome: "success",
    },
  },
  form: {
    label: "com.georgeh.backup",
    command: 'echo "a b" $HOME',
    schedule: { kind: "weekly", weekdays: [1, 3, 5], hour: 9, minute: 0 },
  },
  ...over,
});

export const unmanagedDetail = (): JobDetail => ({
  ...managedDetail(),
  fileName: "com.openai.atlas.update-helper.plist",
  path: "/Users/u/Library/LaunchAgents/com.openai.atlas.update-helper.plist",
  label: "com.openai.atlas.update-helper",
  managed: false,
  state: { loaded: false, disabled: true, pid: null, status: "disabled", printError: null },
  scheduleLines: ["1時間ごと"],
  command: "/Applications/Atlas.app/Helper",
  lastRun: { kind: "estimated", runs: 684, lastExitCode: "0", stdoutMtime: 0, stderrMtime: null },
  form: null,
});
