//! 実際の /bin/bash でラッパーを動かす結合テスト（launchd には触れない）。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use app_lib::domain::wrapper::WRAPPER_SCRIPT;

struct Setup {
    _dir: tempfile::TempDir,
    root: PathBuf,
    wrapper: PathBuf,
    runs: PathBuf,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    // パスにスペースを含めて検証する
    let base = dir.path().join("Application Support").join("launchagent-manager");
    let bin = base.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let wrapper = bin.join("run-job.sh");
    std::fs::write(&wrapper, WRAPPER_SCRIPT).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    Setup { root: dir.path().to_path_buf(), runs: base.join("runs"), wrapper, _dir: dir }
}

fn run(s: &Setup, args: &[&str]) -> i32 {
    Command::new(&s.wrapper).args(args).current_dir(&s.root).status().unwrap().code().unwrap()
}

fn record(s: &Setup, label: &str) -> serde_json::Value {
    let bytes = std::fs::read(s.runs.join(format!("{label}.json"))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn is_iso(v: &serde_json::Value) -> bool {
    let s = v.as_str().unwrap_or("");
    s.len() == 20 && s.as_bytes()[4] == b'-' && s.as_bytes()[10] == b'T' && s.ends_with('Z')
}

#[test]
fn success_records_times_and_exit_code() {
    let s = setup();
    assert_eq!(run(&s, &["com.t.ok", "exit 0"]), 0);
    let r = record(&s, "com.t.ok");
    assert_eq!(r["exit_code"], 0);
    assert_eq!(r["label"], "com.t.ok");
    assert_eq!(r["version"], 1);
    assert!(is_iso(&r["started_at"]), "{r}");
    assert!(is_iso(&r["finished_at"]), "{r}");
}

#[test]
fn failure_exit_code_is_passed_through() {
    let s = setup();
    assert_eq!(run(&s, &["com.t.fail", "exit 3"]), 3);
    assert_eq!(record(&s, "com.t.fail")["exit_code"], 3);
}

#[test]
fn record_while_running_has_null_finished_at() {
    let s = setup();
    let mid = s.root.join("mid.json");
    let cmd = format!("cat \"{}\" > \"{}\"", s.runs.join("com.t.mid.json").display(), mid.display());
    assert_eq!(run(&s, &["com.t.mid", &cmd]), 0);
    let r: serde_json::Value = serde_json::from_slice(&std::fs::read(&mid).unwrap()).unwrap();
    assert!(r["finished_at"].is_null());
    assert!(r["exit_code"].is_null());
    assert!(is_iso(&record(&s, "com.t.mid")["finished_at"]));
}

#[test]
fn special_characters_arrive_as_single_argument() {
    let s = setup();
    let out = s.root.join("out.txt");
    let cmd = format!("printf '%s|' \"a b\" '$HOME' \"日本語\" > \"{}\"", out.display());
    assert_eq!(run(&s, &["com.t.chars", &cmd]), 0);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "a b|$HOME|日本語|");
}

#[test]
fn multiline_command_runs_all_lines() {
    let s = setup();
    let out = s.root.join("lines.txt");
    let cmd = format!("echo 1 > \"{p}\"\necho 2 >> \"{p}\"", p = out.display());
    assert_eq!(run(&s, &["com.t.lines", &cmd]), 0);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "1\n2\n");
}

#[test]
fn invalid_usage_exits_64_without_record() {
    let s = setup();
    assert_eq!(run(&s, &["only-label"]), 64);
    assert_eq!(run(&s, &["../x", "true"]), 64);
    assert!(!Path::new(&s.runs.join("../x.json")).exists());
    assert!(!s.runs.join("only-label.json").exists());
}
