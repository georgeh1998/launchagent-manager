use serde::Deserialize;

use super::config::Config;
use super::dto::{RunOutcome, RunRecordView};
use super::fs::FileSystem;

/// ラッパーが書く実行記録（runs/<Label>.json）。
#[derive(Debug, PartialEq, Deserialize)]
pub struct RunRecord {
    pub version: u32,
    pub label: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
}

pub fn view_of(record: RunRecord) -> RunRecordView {
    let outcome = match (&record.finished_at, record.exit_code) {
        (None, _) | (_, None) => RunOutcome::Running,
        (Some(_), Some(0)) => RunOutcome::Success,
        (Some(_), Some(_)) => RunOutcome::Failure,
    };
    RunRecordView::Record {
        started_at: record.started_at,
        finished_at: record.finished_at,
        exit_code: record.exit_code,
        outcome,
    }
}

pub fn read_run_record(fs: &dyn FileSystem, cfg: &Config, label: &str) -> RunRecordView {
    let path = cfg.run_record_path(label);
    if !fs.exists(&path) {
        return RunRecordView::Never;
    }
    match fs.read(&path) {
        Ok(bytes) => match serde_json::from_slice::<RunRecord>(&bytes) {
            Ok(r) => view_of(r),
            Err(e) => RunRecordView::Corrupt { message: e.to_string() },
        },
        Err(e) => RunRecordView::Corrupt { message: e.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fakes::{FakeFs, Journal};
    use std::path::Path;

    fn setup(content: Option<&str>) -> RunRecordView {
        let cfg = Config::from_home(Path::new("/home/u"), 501);
        let mut fs = FakeFs::new(Journal::default());
        if let Some(c) = content {
            fs = fs.with_file(cfg.run_record_path("a"), c);
        }
        read_run_record(&fs, &cfg, "a")
    }

    fn rec(finished: &str, code: &str) -> String {
        format!(r#"{{"version":1,"label":"a","started_at":"2026-10-09T00:00:00Z","finished_at":{finished},"exit_code":{code},"pid":1}}"#)
    }

    #[test]
    fn never_when_missing() {
        assert_eq!(setup(None), RunRecordView::Never);
    }

    #[test]
    fn success_failure_running() {
        let ok = setup(Some(&rec("\"2026-10-09T00:00:05Z\"", "0")));
        assert_eq!(
            ok,
            RunRecordView::Record {
                started_at: "2026-10-09T00:00:00Z".into(),
                finished_at: Some("2026-10-09T00:00:05Z".into()),
                exit_code: Some(0),
                outcome: RunOutcome::Success
            }
        );
        let RunRecordView::Record { outcome, exit_code, .. } = setup(Some(&rec("\"x\"", "2"))) else { panic!() };
        assert_eq!((outcome, exit_code), (RunOutcome::Failure, Some(2)));
        let RunRecordView::Record { outcome, .. } = setup(Some(&rec("null", "null"))) else { panic!() };
        assert_eq!(outcome, RunOutcome::Running);
    }

    #[test]
    fn corrupt_json() {
        assert!(matches!(setup(Some("{not json")), RunRecordView::Corrupt { .. }));
    }
}
