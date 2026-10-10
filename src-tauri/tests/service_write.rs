mod common;

use std::path::PathBuf;

use app_lib::domain::cmd::CmdOutput;
use app_lib::domain::config::{Config, PLUTIL};
use app_lib::domain::error::AppError;
use app_lib::domain::fakes::{Event, Op};
use app_lib::domain::fs::FileSystem;
use app_lib::domain::plist_model::{job_from_dict, parse_plist_bytes, JobPlist};
use app_lib::domain::schedule::ScheduleInput;
use app_lib::domain::validate::JobInput;
use common::*;
use plist::Value;

fn input(label: &str, command: &str) -> JobInput {
    JobInput { label: label.into(), command: command.into(), schedule: ScheduleInput::Daily { hour: 9, minute: 0 } }
}

fn read_job(e: &Env, file: &str) -> JobPlist {
    job_from_dict(parse_plist_bytes(&e.fs.get(agent(&e.cfg, file)).unwrap()).unwrap())
}

fn la(v: &[&str]) -> Op {
    Op::Launchctl(args(v))
}

fn path_str(c: &Config, file: &str) -> String {
    agent(c, file).to_string_lossy().into_owned()
}

// ---- 追加 ----

#[test]
fn create_writes_plist_and_bootstraps_once() {
    let e = env_with(|fs| fs);
    let name = e.svc.create_job(input("com.example.a", "echo hi")).unwrap();
    assert_eq!(name, "com.example.a.plist");

    let job = read_job(&e, "com.example.a.plist");
    assert_eq!(job.label.as_deref(), Some("com.example.a"));
    assert_eq!(
        job.program_arguments.unwrap(),
        vec![e.cfg.wrapper_path.to_string_lossy().into_owned(), "com.example.a".into(), "echo hi".into()]
    );
    let sci = job.dict["StartCalendarInterval"].as_dictionary().unwrap();
    assert_eq!(sci["Hour"], Value::Integer(9.into()));
    assert_eq!(sci["Minute"], Value::Integer(0.into()));
    assert_eq!(job.stdout_path.as_deref(), Some("/home/u/Library/Logs/com.example.a/stdout.log"));
    assert_eq!(job.stderr_path.as_deref(), Some("/home/u/Library/Logs/com.example.a/stderr.log"));
    assert!(job.dict["EnvironmentVariables"].as_dictionary().unwrap().contains_key("PATH"));

    let p = path_str(&e.cfg, "com.example.a.plist");
    assert_eq!(
        e.journal.mutating_ops(),
        vec![
            Op::WriteFile(agent(&e.cfg, "com.example.a.plist")),
            la(&["enable", "gui/501/com.example.a"]),
            la(&["bootstrap", "gui/501", &p]),
        ]
    );
    let lint: Vec<_> = e.runner.calls_with_program().into_iter().filter(|(p, _)| p == PLUTIL).collect();
    assert_eq!(lint.len(), 1);
    assert!(lint[0].1[1].contains(".com.example.a.plist.tmp-"));
}

#[test]
fn create_makes_separate_log_dirs() {
    let e = env_with(|fs| fs);
    e.svc.create_job(input("com.a", "x")).unwrap();
    e.svc.create_job(input("com.b", "x")).unwrap();
    let mkdirs: Vec<PathBuf> = e
        .journal
        .events()
        .into_iter()
        .filter_map(|ev| if let Event::Mkdir(p) = ev { Some(p) } else { None })
        .collect();
    assert!(mkdirs.contains(&PathBuf::from("/home/u/Library/Logs/com.a")));
    assert!(mkdirs.contains(&PathBuf::from("/home/u/Library/Logs/com.b")));
    assert_ne!(read_job(&e, "com.a.plist").stdout_path, read_job(&e, "com.b.plist").stdout_path);
}

#[test]
fn create_rejects_traversal_label() {
    let e = env_with(|fs| fs);
    let r = e.svc.create_job(input("../evil", "x"));
    assert!(matches!(r, Err(AppError::Validation { .. })));
    assert!(e.journal.events().is_empty());
    assert!(e.fs.paths().iter().all(|p| !p.to_string_lossy().contains("evil")));
}

#[test]
fn create_rejects_duplicate_case_insensitive() {
    let c = cfg();
    let e = env_with(|fs| fs.with_file(agent(&c, "com.x.plist"), plist_xml("<key>Label</key><string>com.x</string>")));
    match e.svc.create_job(input("COM.X", "x")) {
        Err(AppError::Validation { errors }) => assert_eq!(errors[0].field, "label"),
        other => panic!("{other:?}"),
    }
    assert!(e.journal.mutating_ops().is_empty());
}

#[test]
fn create_validation_errors() {
    let e = env_with(|fs| fs);
    let r = e.svc.create_job(JobInput { label: "".into(), command: " ".into(), schedule: ScheduleInput::Daily { hour: 24, minute: 0 } });
    match r {
        Err(AppError::Validation { errors }) => {
            let f: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
            assert_eq!(f, vec!["label", "command", "schedule.hour"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn create_bootstrap_failure_removes_plist() {
    let e = env_with(|fs| fs);
    e.runner.respond(&["bootstrap"], CmdOutput::fail(5, "Bootstrap failed: 5: Input/output error\n"));
    let r = e.svc.create_job(input("com.a", "x"));
    assert_eq!(
        r,
        Err(AppError::Launchctl {
            operation: "bootstrap".into(),
            exit_code: 5,
            stderr: "Bootstrap failed: 5: Input/output error".into(),
            rolled_back: true
        })
    );
    assert!(e.fs.get(agent(&e.cfg, "com.a.plist")).is_none());
}

#[test]
fn create_write_failure_no_bootstrap() {
    let e = env_with(|fs| fs);
    e.fs.fail_write_on(|p| p.to_string_lossy().contains("LaunchAgents"));
    assert!(matches!(e.svc.create_job(input("com.a", "x")), Err(AppError::Io { .. })));
    assert!(e.journal.launchctl_ops().is_empty());
}

#[test]
fn create_lint_failure_cleans_tmp() {
    let e = env_with(|fs| fs);
    e.runner.respond(&["-lint"], CmdOutput { code: 1, stdout: "bad".into(), stderr: String::new() });
    assert!(matches!(e.svc.create_job(input("com.a", "x")), Err(AppError::Plist { .. })));
    assert!(e.fs.get(agent(&e.cfg, "com.a.plist")).is_none());
    assert!(e.fs.paths().iter().all(|p| !p.to_string_lossy().contains(".tmp-")));
    assert!(e.journal.launchctl_ops().is_empty());
}

#[test]
fn create_escapes_special_commands() {
    let e = env_with(|fs| fs);
    let cmd = "echo \"a b\" 'c' $HOME <x>&\n日本語";
    e.svc.create_job(input("com.a", cmd)).unwrap();
    assert_eq!(read_job(&e, "com.a.plist").program_arguments.unwrap()[2], cmd);
}

// ---- 編集 ----

fn existing(extra: &str) -> Env {
    let c = cfg();
    let e = env_with(|fs| fs.with_file(agent(&c, "com.old.plist"), managed_xml(&c, "com.old", "echo old", extra)));
    mark_loaded(&e.runner, "com.old");
    e
}

const EXTRA: &str = "<key>KeepAlive</key><true/><key>WorkingDirectory</key><string>/tmp</string>\
    <key>EnvironmentVariables</key><dict><key>PATH</key><string>/custom</string></dict>";

#[test]
fn update_same_label_order_and_preserves_keys() {
    let e = existing(EXTRA);
    let r = e.svc.update_job("com.old.plist", input("com.old", "echo new")).unwrap();
    assert_eq!(r, "com.old.plist");
    let p = path_str(&e.cfg, "com.old.plist");
    assert_eq!(
        e.journal.mutating_ops(),
        vec![la(&["bootout", "gui/501/com.old"]), Op::WriteFile(agent(&e.cfg, "com.old.plist")), la(&["bootstrap", "gui/501", &p])]
    );
    let job = read_job(&e, "com.old.plist");
    assert_eq!(job.program_arguments.unwrap()[2], "echo new");
    assert_eq!(job.dict["KeepAlive"], Value::Boolean(true));
    assert_eq!(job.dict["WorkingDirectory"].as_string(), Some("/tmp"));
    assert_eq!(job.dict["EnvironmentVariables"].as_dictionary().unwrap()["PATH"].as_string(), Some("/custom"));
}

#[test]
fn update_label_change_order() {
    let e = existing(EXTRA);
    let r = e.svc.update_job("com.old.plist", input("com.new", "echo new")).unwrap();
    assert_eq!(r, "com.new.plist");
    let p = path_str(&e.cfg, "com.new.plist");
    assert_eq!(
        e.journal.mutating_ops(),
        vec![
            la(&["bootout", "gui/501/com.old"]),
            Op::Remove(agent(&e.cfg, "com.old.plist")),
            Op::WriteFile(agent(&e.cfg, "com.new.plist")),
            la(&["enable", "gui/501/com.new"]),
            la(&["bootstrap", "gui/501", &p]),
        ]
    );
    assert!(e.fs.get(agent(&e.cfg, "com.old.plist")).is_none());
    let job = read_job(&e, "com.new.plist");
    assert_eq!(job.program_arguments.as_ref().unwrap()[1], "com.new");
    assert_eq!(job.stdout_path.as_deref(), Some("/home/u/Library/Logs/com.new/stdout.log"));
    assert_eq!(job.dict["KeepAlive"], Value::Boolean(true));
}

#[test]
fn update_bootout_not_loaded_continues() {
    let e = existing("");
    e.runner.respond(&["bootout"], CmdOutput::fail(3, "Boot-out failed: 3: No such process"));
    e.svc.update_job("com.old.plist", input("com.old", "echo new")).unwrap();
    assert_eq!(read_job(&e, "com.old.plist").program_arguments.unwrap()[2], "echo new");
}

#[test]
fn update_not_loaded_skips_bootout() {
    let e = existing("");
    mark_not_loaded(&e.runner, "com.old");
    e.runner.respond(&["bootout"], CmdOutput::fail(5, "Boot-out failed: 5: Input/output error"));
    e.svc.update_job("com.old.plist", input("com.old", "echo new")).unwrap();
    assert!(e.journal.launchctl_ops().iter().all(|a| a[0] != "bootout"));
}

#[test]
fn update_recreates_missing_wrapper() {
    let e = existing("");
    e.fs.remove_file(&e.cfg.wrapper_path).unwrap();
    e.svc.update_job("com.old.plist", input("com.old", "echo new")).unwrap();
    assert!(e.fs.get(&e.cfg.wrapper_path).is_some());
}

#[test]
fn update_bootout_failure_leaves_file_untouched() {
    let e = existing("");
    let before = e.fs.get(agent(&e.cfg, "com.old.plist"));
    e.runner.respond(&["bootout"], CmdOutput::fail(5, "Boot-out failed: 5"));
    assert!(matches!(
        e.svc.update_job("com.old.plist", input("com.old", "echo new")),
        Err(AppError::Launchctl { exit_code: 5, .. })
    ));
    assert_eq!(e.fs.get(agent(&e.cfg, "com.old.plist")), before);
}

#[test]
fn update_write_failure_rebootstraps_old() {
    let e = existing("");
    let before = e.fs.get(agent(&e.cfg, "com.old.plist"));
    e.fs.fail_rename_to(agent(&e.cfg, "com.old.plist"));
    assert!(matches!(e.svc.update_job("com.old.plist", input("com.old", "echo new")), Err(AppError::Io { .. })));
    assert_eq!(e.fs.get(agent(&e.cfg, "com.old.plist")), before);
    let p = path_str(&e.cfg, "com.old.plist");
    assert_eq!(e.journal.mutating_ops(), vec![la(&["bootout", "gui/501/com.old"]), la(&["bootstrap", "gui/501", &p])]);
}

#[test]
fn update_bootstrap_failure_restores_old_content() {
    let e = existing("");
    let before = e.fs.get(agent(&e.cfg, "com.old.plist"));
    e.runner.respond_times(&["bootstrap"], CmdOutput::fail(5, "Bootstrap failed"), 1);
    let r = e.svc.update_job("com.old.plist", input("com.old", "echo new"));
    assert!(matches!(r, Err(AppError::Launchctl { operation, rolled_back: true, .. }) if operation == "bootstrap"));
    assert_eq!(e.fs.get(agent(&e.cfg, "com.old.plist")), before);
    let bootstraps = e.journal.launchctl_ops().into_iter().filter(|a| a[0] == "bootstrap").count();
    assert_eq!(bootstraps, 2);
}

#[test]
fn update_label_change_bootstrap_failure_restores() {
    let e = existing("");
    let before = e.fs.get(agent(&e.cfg, "com.old.plist"));
    e.runner.respond_times(&["bootstrap"], CmdOutput::fail(5, "Bootstrap failed"), 1);
    let r = e.svc.update_job("com.old.plist", input("com.new", "echo new"));
    assert!(matches!(r, Err(AppError::Launchctl { rolled_back: true, .. })));
    assert!(e.fs.get(agent(&e.cfg, "com.new.plist")).is_none());
    assert_eq!(e.fs.get(agent(&e.cfg, "com.old.plist")), before);
    let last = e.journal.launchctl_ops().pop().unwrap();
    assert_eq!(last, args(&["bootstrap", "gui/501", &path_str(&e.cfg, "com.old.plist")]));
}

#[test]
fn update_disabled_job_skips_bootstrap() {
    let e = existing("");
    mark_not_loaded(&e.runner, "com.old");
    e.runner.respond(&["print-disabled"], CmdOutput::ok("\t\"com.old\" => disabled\n"));
    e.svc.update_job("com.old.plist", input("com.old", "echo new")).unwrap();
    assert!(e.journal.launchctl_ops().iter().all(|a| a[0] != "bootstrap"));

    // Label 変更時は disable を引き継ぐ
    e.journal.clear();
    e.svc.update_job("com.old.plist", input("com.new", "echo new")).unwrap();
    assert!(e.journal.launchctl_ops().contains(&args(&["disable", "gui/501/com.new"])));
    assert!(e.journal.launchctl_ops().iter().all(|a| a[0] != "bootstrap"));
}

#[test]
fn update_non_managed_rejected() {
    let c = cfg();
    let e = env_with(|fs| {
        fs.with_file(
            agent(&c, "com.other.plist"),
            plist_xml("<key>Label</key><string>com.other</string><key>ProgramArguments</key><array><string>/bin/x</string></array>"),
        )
    });
    assert!(matches!(e.svc.update_job("com.other.plist", input("com.other", "x")), Err(AppError::NotManaged { .. })));
    assert!(matches!(e.svc.delete_job("com.other.plist"), Err(AppError::NotManaged { .. })));
    assert!(e.journal.events().is_empty());
}

#[test]
fn update_self_label_not_duplicate() {
    let e = existing("");
    assert!(e.svc.update_job("com.old.plist", input("com.old", "x")).is_ok());
}

#[test]
fn update_to_other_existing_label_is_duplicate() {
    let c = cfg();
    let e = existing("");
    e.fs.put(agent(&c, "com.taken.plist"), plist_xml("<key>Label</key><string>com.taken</string>"));
    assert!(matches!(e.svc.update_job("com.old.plist", input("com.taken", "x")), Err(AppError::Validation { .. })));
}

// ---- 削除 ----

#[test]
fn delete_order_and_keeps_logs() {
    let e = existing("");
    e.fs.put(e.cfg.stdout_path("com.old"), "log");
    e.fs.put(e.cfg.run_record_path("com.old"), "{}");
    e.svc.delete_job("com.old.plist").unwrap();
    assert_eq!(e.journal.mutating_ops(), vec![la(&["bootout", "gui/501/com.old"]), Op::Remove(agent(&e.cfg, "com.old.plist"))]);
    assert!(e.fs.get(e.cfg.stdout_path("com.old")).is_some());
    assert!(e.fs.get(e.cfg.run_record_path("com.old")).is_some());
}

#[test]
fn delete_not_loaded_ok() {
    let e = existing("");
    e.runner.respond(&["bootout"], CmdOutput::fail(3, "Boot-out failed: 3: No such process"));
    e.svc.delete_job("com.old.plist").unwrap();
    assert!(e.fs.get(agent(&e.cfg, "com.old.plist")).is_none());
}

#[test]
fn delete_not_loaded_skips_bootout() {
    let e = existing("");
    mark_not_loaded(&e.runner, "com.old");
    e.runner.respond(&["bootout"], CmdOutput::fail(5, "Boot-out failed: 5: Input/output error"));
    e.svc.delete_job("com.old.plist").unwrap();
    assert_eq!(e.journal.mutating_ops(), vec![Op::Remove(agent(&e.cfg, "com.old.plist"))]);
}

#[test]
fn delete_bootout_failure_keeps_file() {
    let e = existing("");
    e.runner.respond(&["bootout"], CmdOutput::fail(1, "nope"));
    assert!(e.svc.delete_job("com.old.plist").is_err());
    assert!(e.fs.get(agent(&e.cfg, "com.old.plist")).is_some());
}

#[test]
fn delete_remove_failure_rebootstraps() {
    let e = existing("");
    e.fs.fail_remove(agent(&e.cfg, "com.old.plist"));
    assert!(matches!(e.svc.delete_job("com.old.plist"), Err(AppError::Io { .. })));
    assert_eq!(e.journal.launchctl_ops().last().unwrap()[0], "bootstrap");
}

// ---- 有効/無効・今すぐ実行 ----

#[test]
fn disable_args() {
    let e = existing("");
    e.svc.set_enabled("com.old.plist", false).unwrap();
    assert_eq!(e.journal.mutating_ops(), vec![la(&["bootout", "gui/501/com.old"]), la(&["disable", "gui/501/com.old"])]);
    assert!(e.fs.get(agent(&e.cfg, "com.old.plist")).is_some());
}

#[test]
fn enable_not_loaded_args() {
    let e = existing("");
    mark_not_loaded(&e.runner, "com.old");
    e.svc.set_enabled("com.old.plist", true).unwrap();
    let p = path_str(&e.cfg, "com.old.plist");
    assert_eq!(e.journal.mutating_ops(), vec![la(&["enable", "gui/501/com.old"]), la(&["bootstrap", "gui/501", &p])]);
}

#[test]
fn enable_loaded_args() {
    let e = existing("");
    e.svc.set_enabled("com.old.plist", true).unwrap();
    assert_eq!(e.journal.mutating_ops(), vec![la(&["enable", "gui/501/com.old"])]);
}

#[test]
fn toggle_and_run_rejected_for_non_managed() {
    let c = cfg();
    let e = env_with(|fs| fs.with_file(agent(&c, "com.other.plist"), plist_xml("<key>Label</key><string>com.other</string><key>Program</key><string>/bin/x</string>")));
    assert!(matches!(e.svc.set_enabled("com.other.plist", false), Err(AppError::NotManaged { .. })));
    assert!(matches!(e.svc.set_enabled("com.other.plist", true), Err(AppError::NotManaged { .. })));
    assert!(matches!(e.svc.run_now("com.other.plist"), Err(AppError::NotManaged { .. })));
    assert!(e.journal.mutating_ops().is_empty());
}

#[test]
fn run_now_args() {
    let e = existing("");
    e.svc.run_now("com.old.plist").unwrap();
    assert_eq!(e.journal.mutating_ops(), vec![la(&["kickstart", "gui/501/com.old"])]);
}

#[test]
fn run_now_not_loaded_error() {
    let e = existing("");
    e.runner.respond(&["kickstart"], CmdOutput::fail(113, NOT_FOUND));
    match e.svc.run_now("com.old.plist") {
        Err(AppError::Launchctl { operation, exit_code, stderr, .. }) => {
            assert_eq!(operation, "kickstart");
            assert_eq!(exit_code, 113);
            assert!(stderr.contains("Could not find service"));
        }
        other => panic!("{other:?}"),
    }
}

/// 実際の launchd を使う確認。`cargo test -- --ignored` で手動実行する。
#[test]
#[ignore]
fn real_launchd_smoke() {
    use app_lib::domain::clock::SystemClock;
    use app_lib::domain::cmd::RealRunner;
    use app_lib::domain::fs::RealFs;
    use app_lib::domain::service::JobService;
    use std::sync::Arc;

    let cfg = Config::from_env().unwrap();
    let svc = JobService::new(Arc::new(RealFs), Arc::new(RealRunner), Arc::new(SystemClock), cfg);
    svc.ensure_wrapper().unwrap();
    let file = svc.create_job(input("dev.georgeh.lam-test", "echo hello")).unwrap();
    let d = svc.get_job_detail(&file).unwrap();
    assert_eq!(d.state.loaded, Some(true));
    svc.delete_job(&file).unwrap();
}
