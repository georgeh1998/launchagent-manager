#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use app_lib::domain::cmd::CmdOutput;
use app_lib::domain::config::Config;
use app_lib::domain::fakes::{FakeFs, FakeRunner, FixedClock, Journal};
use app_lib::domain::service::JobService;

pub const LOADED: &str = include_str!("../fixtures/launchctl/print_loaded.txt");
pub const RUNNING: &str = include_str!("../fixtures/launchctl/print_running.txt");
pub const NOT_FOUND: &str = include_str!("../fixtures/launchctl/print_not_found.stderr.txt");
pub const DISABLED: &str = include_str!("../fixtures/launchctl/print_disabled.txt");
pub const LIST: &str = include_str!("../fixtures/launchctl/list.txt");

pub struct Env {
    pub cfg: Config,
    pub journal: Journal,
    pub fs: Arc<FakeFs>,
    pub runner: Arc<FakeRunner>,
    pub svc: JobService,
}

pub fn cfg() -> Config {
    Config::from_home(Path::new("/home/u"), 501)
}

pub fn env_with(build: impl FnOnce(FakeFs) -> FakeFs) -> Env {
    let cfg = cfg();
    let journal = Journal::default();
    let fs = Arc::new(build(FakeFs::new(journal.clone()).with_dir(&cfg.launch_agents_dir)));
    let runner = Arc::new(FakeRunner::new(journal.clone()));
    let svc = JobService::new(fs.clone(), runner.clone(), Arc::new(FixedClock(1000)), cfg.clone());
    // アプリ起動時と同じくラッパーを配置しておき、その操作はジャーナルから除く
    svc.ensure_wrapper().unwrap();
    journal.clear();
    Env { cfg, journal, fs, runner, svc }
}

pub fn agent(cfg: &Config, file: &str) -> PathBuf {
    cfg.launch_agents_dir.join(file)
}

pub fn plist_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>{body}</dict></plist>"#
    )
}

/// 本アプリ管理ジョブの plist。
pub fn managed_xml(cfg: &Config, label: &str, command: &str, extra: &str) -> String {
    plist_xml(&format!(
        "<key>Label</key><string>{label}</string>\
         <key>ProgramArguments</key><array><string>{}</string><string>{label}</string><string>{command}</string></array>\
         <key>StartCalendarInterval</key><dict><key>Hour</key><integer>9</integer><key>Minute</key><integer>0</integer></dict>\
         <key>StandardOutPath</key><string>{}</string><key>StandardErrorPath</key><string>{}</string>{extra}",
        cfg.wrapper_path.display(),
        cfg.stdout_path(label).display(),
        cfg.stderr_path(label).display(),
    ))
}

pub fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// launchctl list に label を載せ、loaded 扱いにする。
pub fn mark_loaded(runner: &FakeRunner, label: &str) {
    runner.respond(&["print", &format!("gui/501/{label}")], CmdOutput::ok(LOADED));
}

pub fn mark_not_loaded(runner: &FakeRunner, label: &str) {
    runner.respond(&["print", &format!("gui/501/{label}")], CmdOutput::fail(113, NOT_FOUND));
}
