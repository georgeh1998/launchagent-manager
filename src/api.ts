import { invoke } from "@tauri-apps/api/core";
import type { AppError, JobDetail, JobInput, JobSummary } from "./types";

export const listJobs = () => invoke<JobSummary[]>("list_jobs");
export const getJobDetail = (fileName: string) => invoke<JobDetail>("get_job_detail", { fileName });
export const createJob = (input: JobInput) => invoke<string>("create_job", { input });
export const updateJob = (fileName: string, input: JobInput) =>
  invoke<string>("update_job", { fileName, input });
export const deleteJob = (fileName: string) => invoke<null>("delete_job", { fileName });
export const setJobEnabled = (fileName: string, enabled: boolean) =>
  invoke<null>("set_job_enabled", { fileName, enabled });
export const runJobNow = (fileName: string) => invoke<null>("run_job_now", { fileName });

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "kind" in e) return e as AppError;
  if (typeof e === "string") return { kind: "io", message: e };
  if (e instanceof Error) return { kind: "io", message: e.message };
  return { kind: "io", message: String(e) };
}
