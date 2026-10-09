// Rust 側 DTO（src-tauri/src/domain/dto.rs）と同じ形。

export type JobStatus = "enabled" | "disabled" | "unknown";

export interface JobSummary {
  fileName: string;
  path: string;
  label: string | null;
  displayName: string;
  managed: boolean;
  status: JobStatus;
  running: boolean;
  parseError: string | null;
}

export interface JobState {
  loaded: boolean | null;
  disabled: boolean;
  pid: number | null;
  status: JobStatus;
  printError: string | null;
}

export type RunRecordView =
  | { kind: "never" }
  | { kind: "corrupt"; message: string }
  | {
      kind: "record";
      startedAt: string;
      finishedAt: string | null;
      exitCode: number | null;
      outcome: "success" | "failure" | "running";
    };

export type LastRun =
  | { kind: "managed"; record: RunRecordView }
  | {
      kind: "estimated";
      runs: number | null;
      lastExitCode: string | null;
      stdoutMtime: number | null;
      stderrMtime: number | null;
    };

export type ScheduleInput =
  | { kind: "daily"; hour: number; minute: number }
  | { kind: "weekly"; weekdays: number[]; hour: number; minute: number }
  | { kind: "monthly"; day: number; hour: number; minute: number }
  | { kind: "interval"; every: number; unit: "minutes" | "hours" };

export interface JobInput {
  label: string;
  command: string;
  schedule: ScheduleInput;
}

export interface JobFormData {
  label: string;
  command: string;
  schedule: ScheduleInput | null;
}

export interface JobDetail {
  fileName: string;
  path: string;
  label: string | null;
  managed: boolean;
  parseError: string | null;
  state: JobState;
  scheduleLines: string[];
  command: string | null;
  stdoutPath: string | null;
  stderrPath: string | null;
  lastRun: LastRun;
  form: JobFormData | null;
}

export interface FieldError {
  field: string;
  message: string;
}

export type AppError =
  | { kind: "validation"; errors: FieldError[] }
  | { kind: "notManaged"; fileName: string }
  | { kind: "notFound"; fileName: string }
  | { kind: "invalidPath"; path: string }
  | { kind: "launchctl"; operation: string; exitCode: number; stderr: string; rolledBack: boolean }
  | { kind: "io"; message: string }
  | { kind: "plist"; message: string };
