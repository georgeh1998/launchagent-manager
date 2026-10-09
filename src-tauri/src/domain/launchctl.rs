use std::collections::HashMap;
use std::path::Path;

use super::cmd::CommandRunner;
pub use super::cmd::CmdOutput;
use super::config::{Config, LAUNCHCTL, PLUTIL};
use super::error::AppError;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PrintInfo {
    pub state: Option<String>,
    pub pid: Option<u32>,
    pub runs: Option<u64>,
    pub last_exit_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrintResult {
    Loaded(PrintInfo),
    NotLoaded,
    Failed { code: i32, stderr: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListEntry {
    pub pid: Option<u32>,
    pub last_status: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BootoutOutcome {
    Done,
    NotLoaded,
}

pub fn is_not_found(out: &CmdOutput) -> bool {
    out.code == 113 || out.stderr.contains("Could not find service")
}

pub fn is_not_loaded_bootout(out: &CmdOutput) -> bool {
    out.code == 3
        || out.code == 113
        || out.stderr.contains("No such process")
        || out.stderr.contains("Could not find")
}

/// `launchctl print` の出力から必要な項目だけを拾う。
/// 機械向けの出力ではないため、トップレベル（タブ1つ）の `key = value` 行だけを見て、
/// 取れない項目は None にする。
pub fn parse_print(out: &CmdOutput) -> PrintResult {
    if out.code != 0 {
        if is_not_found(out) {
            return PrintResult::NotLoaded;
        }
        return PrintResult::Failed { code: out.code, stderr: out.stderr.trim().to_string() };
    }
    let mut info = PrintInfo::default();
    for line in out.stdout.lines() {
        let Some(rest) = line.strip_prefix('\t') else { continue };
        if rest.starts_with('\t') {
            continue;
        }
        let Some((key, value)) = rest.split_once(" = ") else { continue };
        let value = value.trim();
        match key {
            "state" => info.state = Some(value.to_string()),
            "pid" => info.pid = value.parse().ok(),
            "runs" => info.runs = value.parse().ok(),
            "last exit code" => info.last_exit_code = Some(value.to_string()),
            _ => {}
        }
    }
    PrintResult::Loaded(info)
}

/// `launchctl print-disabled gui/<uid>` の解析。値が true なら disabled。
pub fn parse_print_disabled(stdout: &str) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    for line in stdout.lines() {
        let Some((key, value)) = line.trim().split_once("=>") else { continue };
        let label = key.trim().trim_matches('"');
        let disabled = match value.trim() {
            "disabled" | "true" => true,
            "enabled" | "false" => false,
            _ => continue,
        };
        if !label.is_empty() {
            map.insert(label.to_string(), disabled);
        }
    }
    map
}

/// `launchctl list` の解析（`PID\tStatus\tLabel`）。
pub fn parse_list(stdout: &str) -> HashMap<String, ListEntry> {
    let mut map = HashMap::new();
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 3 || cols[0] == "PID" {
            continue;
        }
        map.insert(
            cols[2].to_string(),
            ListEntry { pid: cols[0].parse().ok(), last_status: cols[1].parse().ok() },
        );
    }
    map
}

pub struct Launchctl<'a> {
    runner: &'a dyn CommandRunner,
    cfg: &'a Config,
}

impl<'a> Launchctl<'a> {
    pub fn new(runner: &'a dyn CommandRunner, cfg: &'a Config) -> Self {
        Launchctl { runner, cfg }
    }

    fn exec(&self, args: Vec<String>) -> Result<CmdOutput, AppError> {
        self.runner.run(LAUNCHCTL, &args).map_err(AppError::from)
    }

    fn check(operation: &str, out: CmdOutput) -> Result<CmdOutput, AppError> {
        if out.code == 0 {
            Ok(out)
        } else {
            Err(AppError::Launchctl {
                operation: operation.into(),
                exit_code: out.code,
                stderr: out.stderr.trim().to_string(),
                rolled_back: false,
            })
        }
    }

    pub fn bootstrap(&self, plist: &Path) -> Result<(), AppError> {
        let out = self.exec(vec!["bootstrap".into(), self.cfg.domain(), plist.to_string_lossy().into()])?;
        Self::check("bootstrap", out).map(|_| ())
    }

    pub fn bootout(&self, label: &str) -> Result<BootoutOutcome, AppError> {
        let out = self.exec(vec!["bootout".into(), self.cfg.service_target(label)])?;
        if out.code == 0 {
            Ok(BootoutOutcome::Done)
        } else if is_not_loaded_bootout(&out) {
            Ok(BootoutOutcome::NotLoaded)
        } else {
            Self::check("bootout", out).map(|_| BootoutOutcome::Done)
        }
    }

    pub fn enable(&self, label: &str) -> Result<(), AppError> {
        let out = self.exec(vec!["enable".into(), self.cfg.service_target(label)])?;
        Self::check("enable", out).map(|_| ())
    }

    pub fn disable(&self, label: &str) -> Result<(), AppError> {
        let out = self.exec(vec!["disable".into(), self.cfg.service_target(label)])?;
        Self::check("disable", out).map(|_| ())
    }

    pub fn kickstart(&self, label: &str) -> Result<(), AppError> {
        let out = self.exec(vec!["kickstart".into(), self.cfg.service_target(label)])?;
        Self::check("kickstart", out).map(|_| ())
    }

    pub fn print(&self, label: &str) -> Result<PrintResult, AppError> {
        let out = self.exec(vec!["print".into(), self.cfg.service_target(label)])?;
        Ok(parse_print(&out))
    }

    pub fn print_disabled(&self) -> Result<HashMap<String, bool>, AppError> {
        let out = self.exec(vec!["print-disabled".into(), self.cfg.domain()])?;
        Self::check("print-disabled", out).map(|o| parse_print_disabled(&o.stdout))
    }

    pub fn list(&self) -> Result<HashMap<String, ListEntry>, AppError> {
        let out = self.exec(vec!["list".into()])?;
        Self::check("list", out).map(|o| parse_list(&o.stdout))
    }
}

pub fn plutil_lint(runner: &dyn CommandRunner, path: &Path) -> Result<(), AppError> {
    let out = runner.run(PLUTIL, &["-lint".into(), path.to_string_lossy().into()])?;
    if out.code == 0 {
        Ok(())
    } else {
        let msg = if out.stderr.trim().is_empty() { out.stdout } else { out.stderr };
        Err(AppError::Plist { message: format!("plist の検証に失敗しました: {}", msg.trim()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fakes::{FakeRunner, Journal};

    const LOADED: &str = include_str!("../../tests/fixtures/launchctl/print_loaded.txt");
    const RUNNING: &str = include_str!("../../tests/fixtures/launchctl/print_running.txt");
    const NOT_FOUND: &str = include_str!("../../tests/fixtures/launchctl/print_not_found.stderr.txt");
    const DISABLED: &str = include_str!("../../tests/fixtures/launchctl/print_disabled.txt");
    const LIST: &str = include_str!("../../tests/fixtures/launchctl/list.txt");

    #[test]
    fn parses_loaded_without_nested_state() {
        let r = parse_print(&CmdOutput::ok(LOADED));
        assert_eq!(
            r,
            PrintResult::Loaded(PrintInfo {
                state: Some("not running".into()),
                pid: None,
                runs: Some(684),
                last_exit_code: Some("0".into()),
            })
        );
    }

    #[test]
    fn parses_running() {
        let PrintResult::Loaded(info) = parse_print(&CmdOutput::ok(RUNNING)) else { panic!() };
        assert_eq!(info.state.as_deref(), Some("running"));
        assert_eq!(info.pid, Some(83124));
        assert_eq!(info.runs, Some(1));
        assert_eq!(info.last_exit_code.as_deref(), Some("(never exited)"));
    }

    #[test]
    fn not_found_is_not_loaded() {
        assert_eq!(parse_print(&CmdOutput::fail(113, NOT_FOUND)), PrintResult::NotLoaded);
        assert_eq!(parse_print(&CmdOutput::fail(1, "boom\n")), PrintResult::Failed { code: 1, stderr: "boom".into() });
    }

    #[test]
    fn garbage_does_not_panic() {
        assert_eq!(parse_print(&CmdOutput::ok("")), PrintResult::Loaded(PrintInfo::default()));
        assert_eq!(parse_print(&CmdOutput::ok("\tpid = abc\n\x00\n=")), PrintResult::Loaded(PrintInfo::default()));
    }

    #[test]
    fn parses_print_disabled() {
        let m = parse_print_disabled(DISABLED);
        assert_eq!(m.get("com.docker.helper"), Some(&false));
        assert_eq!(m.get("com.apple.Siri.agent"), Some(&true));
        assert_eq!(m.len(), 4);
        let m = parse_print_disabled("\t\"a\" => true\n\t\"b\" => false\n");
        assert_eq!(m.get("a"), Some(&true));
        assert_eq!(m.get("b"), Some(&false));
    }

    #[test]
    fn parses_list() {
        let m = parse_list(LIST);
        assert_eq!(m.len(), 3);
        assert_eq!(m["com.openai.atlas.update-helper"], ListEntry { pid: None, last_status: Some(0) });
        assert_eq!(m["com.example.running"].pid, Some(83124));
        assert_eq!(m["com.example.killed"].last_status, Some(-9));
        assert!(!m.contains_key("Label"));
    }

    fn cfg() -> Config {
        Config::from_home(Path::new("/home/u"), 501)
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn client_argument_formats() {
        let runner = FakeRunner::new(Journal::default());
        let c = cfg();
        let l = Launchctl::new(&runner, &c);
        l.bootstrap(Path::new("/home/u/Library/LaunchAgents/a.plist")).unwrap();
        l.bootout("a").unwrap();
        l.enable("a").unwrap();
        l.disable("a").unwrap();
        l.kickstart("a").unwrap();
        l.print("a").unwrap();
        l.print_disabled().unwrap();
        l.list().unwrap();
        plutil_lint(&runner, Path::new("/x.plist")).unwrap();
        let calls = runner.calls_with_program();
        assert!(calls[..8].iter().all(|(p, _)| p == LAUNCHCTL));
        let args: Vec<Vec<String>> = calls.into_iter().map(|(_, a)| a).collect();
        assert_eq!(
            args,
            vec![
                s(&["bootstrap", "gui/501", "/home/u/Library/LaunchAgents/a.plist"]),
                s(&["bootout", "gui/501/a"]),
                s(&["enable", "gui/501/a"]),
                s(&["disable", "gui/501/a"]),
                s(&["kickstart", "gui/501/a"]),
                s(&["print", "gui/501/a"]),
                s(&["print-disabled", "gui/501"]),
                s(&["list"]),
                s(&["-lint", "/x.plist"]),
            ]
        );
    }

    #[test]
    fn bootout_not_loaded_and_kickstart_failure() {
        let runner = FakeRunner::new(Journal::default());
        runner.respond(&["bootout"], CmdOutput::fail(3, "Boot-out failed: 3: No such process"));
        runner.respond(&["kickstart"], CmdOutput::fail(113, NOT_FOUND));
        let c = cfg();
        let l = Launchctl::new(&runner, &c);
        assert_eq!(l.bootout("a").unwrap(), BootoutOutcome::NotLoaded);
        match l.kickstart("a") {
            Err(AppError::Launchctl { operation, exit_code: 113, .. }) => assert_eq!(operation, "kickstart"),
            other => panic!("{other:?}"),
        }
        runner.respond(&["bootout"], CmdOutput::fail(5, "Boot-out failed: 5: Input/output error"));
        assert!(matches!(l.bootout("a"), Err(AppError::Launchctl { exit_code: 5, .. })));
    }
}
