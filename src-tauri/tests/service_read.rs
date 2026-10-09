mod common;

use app_lib::domain::cmd::CmdOutput;
use app_lib::domain::dto::{JobStatus, LastRun, RunOutcome, RunRecordView};
use app_lib::domain::error::AppError;
use app_lib::domain::schedule::ScheduleInput;
use common::*;
use plist::{Dictionary, Value};

fn binary_plist(label: &str) -> Vec<u8> {
    let mut d = Dictionary::new();
    d.insert("Label".into(), Value::String(label.into()));
    d.insert("Program".into(), Value::String("/bin/true".into()));
    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &Value::Dictionary(d)).unwrap();
    buf
}

fn mixed_env() -> Env {
    let c = cfg();
    let e = env_with(|fs| {
        fs.with_file(agent(&c, "com.openai.atlas.update-helper.plist"), plist_xml("<key>Label</key><string>com.openai.atlas.update-helper</string><key>StartInterval</key><integer>3600</integer>"))
            .with_file(agent(&c, "com.example.bin.plist"), binary_plist("com.example.bin"))
            .with_file(agent(&c, "broken.plist"), "<?xml version=\"1.0\"?><plist><dict><key>")
            .with_file(agent(&c, "nolabel.plist"), plist_xml("<key>Program</key><string>/bin/true</string>"))
            .with_file(agent(&c, ".DS_Store"), [0u8, 0, 0, 1, 66, 117, 100, 49])
            .with_file(agent(&c, "foo.txt"), "hello")
            .with_dir(agent(&c, "sub.plist"))
            .with_file(agent(&c, "com.georgeh.managed.plist"), managed_xml(&c, "com.georgeh.managed", "echo hi", ""))
    });
    e.runner.respond(&["list"], CmdOutput::ok(LIST));
    e.runner.respond(&["print-disabled"], CmdOutput::ok("\t\"com.example.bin\" => disabled\n"));
    e
}

#[test]
fn list_survives_mixed_directory() {
    let e = mixed_env();
    let rows = e.svc.list_jobs().unwrap();
    let names: Vec<&str> = rows.iter().map(|r| r.display_name.as_str()).collect();
    assert_eq!(
        names,
        vec!["broken.plist", "com.example.bin", "com.georgeh.managed", "com.openai.atlas.update-helper", "nolabel.plist"]
    );
    let broken = rows.iter().find(|r| r.file_name == "broken.plist").unwrap();
    assert!(broken.parse_error.is_some());
    assert_eq!(broken.status, JobStatus::Unknown);
    let nolabel = rows.iter().find(|r| r.file_name == "nolabel.plist").unwrap();
    assert_eq!(nolabel.label, None);
    assert!(nolabel.parse_error.is_some());
}

#[test]
fn list_status_and_managed() {
    let e = mixed_env();
    let rows = e.svc.list_jobs().unwrap();
    let get = |n: &str| rows.iter().find(|r| r.display_name == n).unwrap().clone();
    let atlas = get("com.openai.atlas.update-helper");
    assert_eq!(atlas.status, JobStatus::Enabled);
    assert!(!atlas.managed);
    assert!(!atlas.running);
    assert_eq!(get("com.example.bin").status, JobStatus::Disabled);
    let managed = get("com.georgeh.managed");
    assert!(managed.managed);
    assert_eq!(managed.status, JobStatus::Disabled, "list に無い → 無効");
}

#[test]
fn list_unknown_when_launchctl_fails() {
    let e = mixed_env();
    e.runner.respond(&["list"], CmdOutput::fail(1, "boom"));
    let rows = e.svc.list_jobs().unwrap();
    assert!(rows.iter().all(|r| r.status == JobStatus::Unknown));
}

#[test]
fn list_running_flag() {
    let c = cfg();
    let e = env_with(|fs| fs.with_file(agent(&c, "com.example.running.plist"), plist_xml("<key>Label</key><string>com.example.running</string>")));
    e.runner.respond(&["list"], CmdOutput::ok(LIST));
    let rows = e.svc.list_jobs().unwrap();
    assert!(rows[0].running);
    assert_eq!(rows[0].status, JobStatus::Enabled);
}

fn managed_env(record: Option<&str>) -> Env {
    let c = cfg();
    let e = env_with(|fs| {
        let fs = fs.with_file(
            agent(&c, "com.georgeh.backup.plist"),
            managed_xml(&c, "com.georgeh.backup", "echo &quot;a b&quot; $HOME", "<key>RunAtLoad</key><true/>"),
        );
        match record {
            Some(r) => fs.with_file(c.run_record_path("com.georgeh.backup"), r),
            None => fs,
        }
    });
    mark_loaded(&e.runner, "com.georgeh.backup");
    e
}

#[test]
fn detail_managed_job() {
    let rec = r#"{"version":1,"label":"com.georgeh.backup","started_at":"2026-10-09T00:00:00Z","finished_at":"2026-10-09T00:00:05Z","exit_code":0,"pid":1}"#;
    let e = managed_env(Some(rec));
    let d = e.svc.get_job_detail("com.georgeh.backup.plist").unwrap();
    assert!(d.managed);
    assert_eq!(d.label.as_deref(), Some("com.georgeh.backup"));
    assert_eq!(d.path, "/home/u/Library/LaunchAgents/com.georgeh.backup.plist");
    assert_eq!(d.schedule_lines, vec!["毎日 09:00", "ログイン時に実行"]);
    assert_eq!(d.command.as_deref(), Some("echo \"a b\" $HOME"));
    assert_eq!(d.stdout_path.as_deref(), Some("/home/u/Library/Logs/com.georgeh.backup/stdout.log"));
    assert_eq!(d.state.loaded, Some(true));
    assert_eq!(d.state.status, JobStatus::Enabled);
    let form = d.form.unwrap();
    assert_eq!(form.command, "echo \"a b\" $HOME");
    assert_eq!(form.schedule, Some(ScheduleInput::Daily { hour: 9, minute: 0 }));
    match d.last_run {
        LastRun::Managed { record: RunRecordView::Record { outcome, exit_code, .. } } => {
            assert_eq!(outcome, RunOutcome::Success);
            assert_eq!(exit_code, Some(0));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn detail_managed_record_variants() {
    let cases = [
        (Some(r#"{"version":1,"label":"x","started_at":"s","finished_at":"f","exit_code":2,"pid":1}"#), "failure"),
        (Some(r#"{"version":1,"label":"x","started_at":"s","finished_at":null,"exit_code":null,"pid":1}"#), "running"),
        (None, "never"),
        (Some("{broken"), "corrupt"),
    ];
    for (rec, expected) in cases {
        let e = managed_env(rec);
        let LastRun::Managed { record } = e.svc.get_job_detail("com.georgeh.backup.plist").unwrap().last_run else {
            panic!()
        };
        let kind = match record {
            RunRecordView::Never => "never",
            RunRecordView::Corrupt { .. } => "corrupt",
            RunRecordView::Record { outcome: RunOutcome::Failure, .. } => "failure",
            RunRecordView::Record { outcome: RunOutcome::Running, .. } => "running",
            RunRecordView::Record { .. } => "success",
        };
        assert_eq!(kind, expected);
    }
}

#[test]
fn detail_unmanaged_job_estimates() {
    let c = cfg();
    let e = env_with(|fs| {
        fs.with_file(
            agent(&c, "com.openai.atlas.update-helper.plist"),
            plist_xml("<key>Label</key><string>com.openai.atlas.update-helper</string><key>ProgramArguments</key><array><string>/bin/x</string><string>a b</string></array><key>StandardOutPath</key><string>/home/u/Library/Logs/x.log</string>"),
        )
        .with_file("/home/u/Library/Logs/x.log", "log")
        .with_mtime("/home/u/Library/Logs/x.log", 1_700_000_000)
    });
    mark_loaded(&e.runner, "com.openai.atlas.update-helper");
    let d = e.svc.get_job_detail("com.openai.atlas.update-helper.plist").unwrap();
    assert!(!d.managed);
    assert!(d.form.is_none());
    assert_eq!(d.command.as_deref(), Some("/bin/x 'a b'"));
    assert_eq!(d.stderr_path, None);
    assert_eq!(d.schedule_lines, vec!["スケジュール指定なし"]);
    assert_eq!(
        d.last_run,
        LastRun::Estimated { runs: Some(684), last_exit_code: Some("0".into()), stdout_mtime: Some(1_700_000_000), stderr_mtime: None }
    );
}

#[test]
fn detail_running_pid() {
    let c = cfg();
    let e = env_with(|fs| fs.with_file(agent(&c, "com.example.running.plist"), plist_xml("<key>Label</key><string>com.example.running</string>")));
    e.runner.respond(&["print", "gui/501/com.example.running"], CmdOutput::ok(RUNNING));
    let d = e.svc.get_job_detail("com.example.running.plist").unwrap();
    assert_eq!(d.state.pid, Some(83124));
    assert_eq!(d.last_run, LastRun::Estimated { runs: Some(1), last_exit_code: Some("(never exited)".into()), stdout_mtime: None, stderr_mtime: None });
}

#[test]
fn detail_not_loaded_and_print_failure() {
    let e = managed_env(None);
    mark_not_loaded(&e.runner, "com.georgeh.backup");
    let d = e.svc.get_job_detail("com.georgeh.backup.plist").unwrap();
    assert_eq!(d.state.loaded, Some(false));
    assert_eq!(d.state.status, JobStatus::Disabled);

    e.runner.respond(&["print", "gui/501/com.georgeh.backup"], CmdOutput::fail(1, "weird error"));
    let d = e.svc.get_job_detail("com.georgeh.backup.plist").unwrap();
    assert_eq!(d.state.loaded, None);
    assert_eq!(d.state.status, JobStatus::Unknown);
    assert_eq!(d.state.print_error.as_deref(), Some("終了コード 1: weird error"));
}

#[test]
fn detail_disabled_flag() {
    let e = managed_env(None);
    e.runner.respond(&["print-disabled"], CmdOutput::ok("\t\"com.georgeh.backup\" => disabled\n"));
    let d = e.svc.get_job_detail("com.georgeh.backup.plist").unwrap();
    assert!(d.state.disabled);
    assert_eq!(d.state.status, JobStatus::Disabled);
}

#[test]
fn detail_parse_error_is_not_err() {
    let e = mixed_env();
    let d = e.svc.get_job_detail("broken.plist").unwrap();
    assert!(d.parse_error.is_some());
    assert!(!d.managed);
    let d = e.svc.get_job_detail("nolabel.plist").unwrap();
    assert!(d.parse_error.is_some());
    assert_eq!(d.command.as_deref(), Some("/bin/true"));
}

#[test]
fn detail_rejects_invalid_paths() {
    let e = mixed_env();
    for bad in ["../x.plist", "a/b.plist", ".hidden.plist", "x.txt", ""] {
        assert!(matches!(e.svc.get_job_detail(bad), Err(AppError::InvalidPath { .. })), "{bad}");
    }
    assert!(matches!(e.svc.get_job_detail("missing.plist"), Err(AppError::NotFound { .. })));
}
