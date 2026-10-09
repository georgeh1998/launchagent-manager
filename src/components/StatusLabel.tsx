import { statusLabel } from "../format";
import type { JobStatus } from "../types";

export function StatusLabel({ status }: { status: JobStatus }) {
  return <span className={`status-label status-label--${status}`}>{statusLabel(status)}</span>;
}
