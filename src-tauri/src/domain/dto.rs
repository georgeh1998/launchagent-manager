//! フロントエンドとやり取りする DTO（serde は camelCase）。

use serde::Serialize;

use super::schedule::ScheduleInput;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Enabled,
    Disabled,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub file_name: String,
    pub path: String,
    pub label: Option<String>,
    pub display_name: String,
    pub managed: bool,
    pub status: JobStatus,
    pub running: bool,
    pub parse_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    /// None = launchctl から取得できなかった
    pub loaded: Option<bool>,
    pub disabled: bool,
    pub pid: Option<u32>,
    pub status: JobStatus,
    pub print_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunOutcome {
    Success,
    Failure,
    Running,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RunRecordView {
    Never,
    Corrupt { message: String },
    Record { started_at: String, finished_at: Option<String>, exit_code: Option<i32>, outcome: RunOutcome },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LastRun {
    Managed { record: RunRecordView },
    Estimated {
        runs: Option<u64>,
        last_exit_code: Option<String>,
        stdout_mtime: Option<i64>,
        stderr_mtime: Option<i64>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFormData {
    pub label: String,
    pub command: String,
    pub schedule: Option<ScheduleInput>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDetail {
    pub file_name: String,
    pub path: String,
    pub label: Option<String>,
    pub managed: bool,
    pub parse_error: Option<String>,
    pub state: JobState,
    pub schedule_lines: Vec<String>,
    pub command: Option<String>,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
    pub last_run: LastRun,
    pub form: Option<JobFormData>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_run_json_shape() {
        let v = LastRun::Managed {
            record: RunRecordView::Record {
                started_at: "s".into(),
                finished_at: None,
                exit_code: None,
                outcome: RunOutcome::Running,
            },
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({"kind":"managed","record":{"kind":"record","startedAt":"s","finishedAt":null,"exitCode":null,"outcome":"running"}})
        );
        let v = LastRun::Estimated { runs: Some(1), last_exit_code: None, stdout_mtime: None, stderr_mtime: Some(3) };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({"kind":"estimated","runs":1,"lastExitCode":null,"stdoutMtime":null,"stderrMtime":3})
        );
    }
}
