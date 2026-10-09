use std::path::{Path, PathBuf};
use std::sync::Arc;

use plist::{Dictionary, Value};

use super::clock::Clock;
use super::cmd::CommandRunner;
use super::config::{Config, DEFAULT_PATH_ENV};
use super::dto::{JobDetail, JobFormData, JobState, JobStatus, JobSummary, LastRun};
use super::error::{AppError, FieldError};
use super::fs::FileSystem;
use super::launchctl::{plutil_lint, BootoutOutcome, Launchctl, PrintInfo, PrintResult};
use super::plist_model::{
    display_command, is_managed, job_from_dict, managed_command, parse_plist_bytes, serialize_xml, JobPlist,
};
use super::run_record::read_run_record;
use super::schedule::{apply_schedule, describe, normalize_weekdays, to_form, ScheduleInput};
use super::validate::{label_format_error, validate_input, JobInput, MSG_LABEL_DUP};
use super::wrapper::ensure_wrapper;

const MSG_NO_LABEL: &str = "Label がありません";

struct ScannedJob {
    file_name: String,
    path: PathBuf,
    job: Result<JobPlist, String>,
}

/// ジョブの一覧・詳細取得と追加・編集・削除などの操作。
/// 副作用はすべて FileSystem / CommandRunner / Clock 経由で行う。
pub struct JobService {
    fs: Arc<dyn FileSystem>,
    runner: Arc<dyn CommandRunner>,
    clock: Arc<dyn Clock>,
    cfg: Config,
}

fn plist_err(message: impl Into<String>) -> AppError {
    AppError::Plist { message: message.into() }
}

fn normalize_input(mut input: JobInput) -> JobInput {
    if let ScheduleInput::Weekly { weekdays, .. } = &mut input.schedule {
        if weekdays.iter().all(|d| (0..=7).contains(d)) {
            *weekdays = normalize_weekdays(weekdays);
        }
    }
    input
}

fn status_of(loaded: Option<bool>, disabled: Option<bool>) -> JobStatus {
    match (loaded, disabled) {
        (Some(true), Some(false)) => JobStatus::Enabled,
        (Some(_), Some(_)) => JobStatus::Disabled,
        _ => JobStatus::Unknown,
    }
}

/// 管理ジョブの plist を組み立てる。`base` があれば 3 項目以外のキーを保持する。
pub fn build_managed_plist(base: Option<Dictionary>, input: &JobInput, old_label: Option<&str>, cfg: &Config) -> Dictionary {
    let mut dict = base.unwrap_or_default();
    dict.insert("Label".into(), Value::String(input.label.clone()));
    // Program があるとラッパーを迂回するため削除する（順序を保つため retain）
    dict.retain(|k, _| k != "Program");
    dict.insert(
        "ProgramArguments".into(),
        Value::Array(vec![
            Value::String(cfg.wrapper_path.to_string_lossy().into()),
            Value::String(input.label.clone()),
            Value::String(input.command.clone()),
        ]),
    );
    apply_schedule(&mut dict, &input.schedule);

    let log_paths = [
        ("StandardOutPath", cfg.stdout_path(&input.label), old_label.map(|o| cfg.stdout_path(o))),
        ("StandardErrorPath", cfg.stderr_path(&input.label), old_label.map(|o| cfg.stderr_path(o))),
    ];
    for (key, new_path, old_auto) in log_paths {
        let current = dict.get(key).and_then(Value::as_string).map(PathBuf::from);
        let replace = match (&current, &old_auto) {
            (None, _) => true,
            (Some(cur), Some(old)) => cur == old,
            (Some(_), None) => false,
        };
        if replace {
            dict.insert(key.into(), Value::String(new_path.to_string_lossy().into()));
        }
    }

    match dict.get_mut("EnvironmentVariables") {
        Some(Value::Dictionary(env)) => {
            if !env.contains_key("PATH") {
                env.insert("PATH".into(), Value::String(DEFAULT_PATH_ENV.into()));
            }
        }
        Some(_) => {}
        None => {
            let mut env = Dictionary::new();
            env.insert("PATH".into(), Value::String(DEFAULT_PATH_ENV.into()));
            dict.insert("EnvironmentVariables".into(), Value::Dictionary(env));
        }
    }
    dict
}

impl JobService {
    pub fn new(fs: Arc<dyn FileSystem>, runner: Arc<dyn CommandRunner>, clock: Arc<dyn Clock>, cfg: Config) -> Self {
        JobService { fs, runner, clock, cfg }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    fn lc(&self) -> Launchctl<'_> {
        Launchctl::new(self.runner.as_ref(), &self.cfg)
    }

    pub fn ensure_wrapper(&self) -> Result<(), AppError> {
        ensure_wrapper(self.fs.as_ref(), self.clock.as_ref(), &self.cfg)
    }

    // ---- 読み取り ----

    /// フロントから受け取ったファイル名を LaunchAgents 直下のパスに解決する。
    fn resolve_file(&self, file_name: &str) -> Result<PathBuf, AppError> {
        let invalid = || AppError::InvalidPath { path: file_name.to_string() };
        if file_name.is_empty()
            || file_name.contains('/')
            || file_name.contains('\0')
            || file_name.starts_with('.')
            || !file_name.ends_with(".plist")
        {
            return Err(invalid());
        }
        let path = self.cfg.launch_agents_dir.join(file_name);
        if path.parent() != Some(self.cfg.launch_agents_dir.as_path()) {
            return Err(invalid());
        }
        if !self.fs.exists(&path) {
            return Err(AppError::NotFound { file_name: file_name.to_string() });
        }
        Ok(path)
    }

    fn parse_job(&self, path: &Path) -> Result<Result<JobPlist, String>, AppError> {
        let bytes = self.fs.read(path)?;
        Ok(parse_plist_bytes(&bytes).map(job_from_dict))
    }

    fn load_job(&self, path: &Path) -> Result<JobPlist, AppError> {
        self.parse_job(path)?.map_err(plist_err)
    }

    fn scan(&self) -> Result<Vec<ScannedJob>, AppError> {
        if !self.fs.exists(&self.cfg.launch_agents_dir) {
            return Ok(vec![]);
        }
        let mut jobs = Vec::new();
        for entry in self.fs.list_dir(&self.cfg.launch_agents_dir)? {
            if !entry.is_file {
                continue;
            }
            let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else { continue };
            if name.starts_with('.') || !name.ends_with(".plist") {
                continue;
            }
            let job = match self.parse_job(&entry.path) {
                Ok(j) => j,
                Err(e) => Err(e.to_string()),
            };
            jobs.push(ScannedJob { file_name: name.to_string(), path: entry.path.clone(), job });
        }
        Ok(jobs)
    }

    /// 重複判定用に、既存ジョブの Label とファイル名（拡張子なし）を集める。
    fn existing_labels(&self, exclude_file: Option<&str>) -> Result<Vec<String>, AppError> {
        let mut labels = Vec::new();
        for s in self.scan()? {
            if Some(s.file_name.as_str()) == exclude_file {
                continue;
            }
            labels.push(s.file_name.trim_end_matches(".plist").to_string());
            if let Ok(Some(l)) = s.job.as_ref().map(|j| j.label.clone()) {
                labels.push(l);
            }
        }
        Ok(labels)
    }

    pub fn list_jobs(&self) -> Result<Vec<JobSummary>, AppError> {
        let scanned = self.scan()?;
        let lc = self.lc();
        let list = lc.list().ok();
        let disabled = lc.print_disabled().ok();
        let mut rows: Vec<JobSummary> = scanned
            .into_iter()
            .map(|s| {
                let path = s.path.to_string_lossy().into_owned();
                match s.job {
                    Ok(job) => {
                        let managed = is_managed(&job, &self.cfg.wrapper_path);
                        let (status, running) = match &job.label {
                            Some(l) => {
                                let entry = list.as_ref().map(|m| m.get(l));
                                let loaded = entry.map(|e| e.is_some());
                                let dis = disabled.as_ref().map(|m| m.get(l).copied().unwrap_or(false));
                                let running = entry.flatten().map(|e| e.pid.is_some()).unwrap_or(false);
                                (status_of(loaded, dis), running)
                            }
                            None => (JobStatus::Unknown, false),
                        };
                        JobSummary {
                            display_name: job.label.clone().unwrap_or_else(|| s.file_name.clone()),
                            parse_error: if job.label.is_none() { Some(MSG_NO_LABEL.into()) } else { None },
                            file_name: s.file_name,
                            path,
                            label: job.label,
                            managed,
                            status,
                            running,
                        }
                    }
                    Err(e) => JobSummary {
                        display_name: s.file_name.clone(),
                        file_name: s.file_name,
                        path,
                        label: None,
                        managed: false,
                        status: JobStatus::Unknown,
                        running: false,
                        parse_error: Some(e),
                    },
                }
            })
            .collect();
        rows.sort_by_key(|r| r.display_name.to_lowercase());
        Ok(rows)
    }

    pub fn get_job_detail(&self, file_name: &str) -> Result<JobDetail, AppError> {
        let path = self.resolve_file(file_name)?;
        let path_str = path.to_string_lossy().into_owned();
        let job = match self.parse_job(&path)? {
            Ok(j) => j,
            Err(e) => {
                return Ok(JobDetail {
                    file_name: file_name.to_string(),
                    path: path_str,
                    label: None,
                    managed: false,
                    parse_error: Some(e),
                    state: JobState { loaded: None, disabled: false, pid: None, status: JobStatus::Unknown, print_error: None },
                    schedule_lines: vec![],
                    command: None,
                    stdout_path: None,
                    stderr_path: None,
                    last_run: LastRun::Estimated { runs: None, last_exit_code: None, stdout_mtime: None, stderr_mtime: None },
                    form: None,
                });
            }
        };
        let managed = is_managed(&job, &self.cfg.wrapper_path);
        let lc = self.lc();

        let mut info = PrintInfo::default();
        let state = match &job.label {
            Some(label) => {
                let (loaded, print_error) = match lc.print(label)? {
                    PrintResult::Loaded(i) => {
                        info = i;
                        (Some(true), None)
                    }
                    PrintResult::NotLoaded => (Some(false), None),
                    PrintResult::Failed { code, stderr } => (None, Some(format!("終了コード {code}: {stderr}"))),
                };
                let dis = lc.print_disabled().ok().map(|m| m.get(label).copied().unwrap_or(false));
                JobState {
                    loaded,
                    disabled: dis.unwrap_or(false),
                    pid: info.pid,
                    status: status_of(loaded, dis),
                    print_error,
                }
            }
            None => JobState { loaded: None, disabled: false, pid: None, status: JobStatus::Unknown, print_error: None },
        };

        let mtime = |p: &Option<String>| p.as_ref().and_then(|p| self.fs.modified_epoch_secs(Path::new(p)).ok().flatten());
        let last_run = match (&job.label, managed) {
            (Some(label), true) => LastRun::Managed { record: read_run_record(self.fs.as_ref(), &self.cfg, label) },
            _ => LastRun::Estimated {
                runs: info.runs,
                last_exit_code: info.last_exit_code.clone(),
                stdout_mtime: mtime(&job.stdout_path),
                stderr_mtime: mtime(&job.stderr_path),
            },
        };
        let form = match (&job.label, managed) {
            (Some(label), true) => Some(JobFormData {
                label: label.clone(),
                command: managed_command(&job),
                schedule: to_form(&job.schedule),
            }),
            _ => None,
        };

        Ok(JobDetail {
            file_name: file_name.to_string(),
            path: path_str,
            label: job.label.clone(),
            managed,
            parse_error: if job.label.is_none() { Some(MSG_NO_LABEL.into()) } else { None },
            state,
            schedule_lines: describe(&job.schedule),
            command: if managed { Some(managed_command(&job)) } else { display_command(&job) },
            stdout_path: job.stdout_path.clone(),
            stderr_path: job.stderr_path.clone(),
            last_run,
            form,
        })
    }

    // ---- 書き込み ----

    /// Label から LaunchAgents 直下の plist パスを作る。ディレクトリ外を指すものは拒否する。
    fn plist_path_for(&self, label: &str) -> Result<PathBuf, AppError> {
        if label_format_error(label).is_some() {
            return Err(AppError::InvalidPath { path: label.to_string() });
        }
        let path = self.cfg.launch_agents_dir.join(format!("{label}.plist"));
        if path.parent() != Some(self.cfg.launch_agents_dir.as_path()) {
            return Err(AppError::InvalidPath { path: path.to_string_lossy().into() });
        }
        Ok(path)
    }

    /// 同じディレクトリの一時ファイルに書いてから rename する。`lint` なら plutil -lint で検証する。
    fn write_bytes_atomic(&self, path: &Path, bytes: &[u8], lint: bool) -> Result<(), AppError> {
        let dir = path.parent().ok_or_else(|| AppError::InvalidPath { path: path.to_string_lossy().into() })?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("job.plist");
        let tmp = dir.join(format!(".{name}.tmp-{}", self.clock.now_millis()));
        self.fs.write(&tmp, bytes)?;
        if lint {
            if let Err(e) = plutil_lint(self.runner.as_ref(), &tmp) {
                let _ = self.fs.remove_file(&tmp);
                return Err(e);
            }
        }
        if let Err(e) = self.fs.rename(&tmp, path) {
            let _ = self.fs.remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    fn write_plist_atomic(&self, path: &Path, dict: &Dictionary) -> Result<(), AppError> {
        let bytes = serialize_xml(dict).map_err(plist_err)?;
        self.write_bytes_atomic(path, &bytes, true)
    }

    fn validate(&self, input: &JobInput, exclude_file: Option<&str>, original: Option<&str>) -> Result<(), AppError> {
        let existing = self.existing_labels(exclude_file)?;
        let errors = validate_input(input, &existing, original);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation { errors })
        }
    }

    fn prepare_dirs(&self, label: &str) -> Result<(), AppError> {
        self.fs.create_dir_all(&self.cfg.job_log_dir(label))?;
        self.fs.create_dir_all(&self.cfg.runs_dir)?;
        Ok(())
    }

    /// 追加: 書き込み → enable → bootstrap。失敗したら書いた plist を削除する。
    pub fn create_job(&self, input: JobInput) -> Result<String, AppError> {
        let input = normalize_input(input);
        self.validate(&input, None, None)?;
        let path = self.plist_path_for(&input.label)?;
        if self.fs.exists(&path) {
            return Err(AppError::Validation { errors: vec![FieldError::new("label", MSG_LABEL_DUP)] });
        }
        self.ensure_wrapper()?;
        self.prepare_dirs(&input.label)?;
        let dict = build_managed_plist(None, &input, None, &self.cfg);
        self.write_plist_atomic(&path, &dict)?;
        let lc = self.lc();
        if let Err(e) = lc.enable(&input.label).and_then(|_| lc.bootstrap(&path)) {
            let removed = self.fs.remove_file(&path).is_ok();
            return Err(e.with_rolled_back(removed));
        }
        Ok(format!("{}.plist", input.label))
    }

    fn load_managed(&self, file_name: &str) -> Result<(PathBuf, Vec<u8>, JobPlist, String), AppError> {
        let path = self.resolve_file(file_name)?;
        let bytes = self.fs.read(&path)?;
        let job = job_from_dict(parse_plist_bytes(&bytes).map_err(plist_err)?);
        if !is_managed(&job, &self.cfg.wrapper_path) {
            return Err(AppError::NotManaged { file_name: file_name.to_string() });
        }
        let label = job.label.clone().ok_or_else(|| plist_err(MSG_NO_LABEL))?;
        Ok((path, bytes, job, label))
    }

    /// 現在の (loaded, disabled)。取得できなければエラーにして何も変更しない。
    fn current_state(&self, label: &str) -> Result<(bool, bool), AppError> {
        let lc = self.lc();
        let loaded = match lc.print(label)? {
            PrintResult::Loaded(_) => true,
            PrintResult::NotLoaded => false,
            PrintResult::Failed { code, stderr } => {
                return Err(AppError::Launchctl { operation: "print".into(), exit_code: code, stderr, rolled_back: false })
            }
        };
        let disabled = lc.print_disabled()?.get(label).copied().unwrap_or(false);
        Ok((loaded, disabled))
    }

    /// 編集: bootout → 書き換え → bootstrap。失敗したら旧内容を書き戻して旧ジョブを再登録する。
    pub fn update_job(&self, file_name: &str, input: JobInput) -> Result<String, AppError> {
        let (path, old_bytes, job, old_label) = self.load_managed(file_name)?;
        let input = normalize_input(input);
        self.validate(&input, Some(file_name), Some(&old_label))?;
        let new_path = self.plist_path_for(&input.label)?;
        let (was_loaded, was_disabled) = self.current_state(&old_label)?;
        let was_enabled = was_loaded && !was_disabled;
        let new_dict = build_managed_plist(Some(job.dict), &input, Some(&old_label), &self.cfg);
        let lc = self.lc();
        self.ensure_wrapper()?;
        self.prepare_dirs(&input.label)?;

        // 未登録のジョブには bootout しない（macOS によって終了コードが異なるため）
        let bootout_old = || -> Result<(), AppError> {
            if was_loaded {
                lc.bootout(&old_label)?;
            }
            Ok(())
        };

        if input.label == old_label {
            bootout_old()?;
            if let Err(e) = self.write_plist_atomic(&path, &new_dict) {
                if was_loaded {
                    let _ = lc.bootstrap(&path);
                }
                return Err(e);
            }
            if was_enabled {
                if let Err(e) = lc.bootstrap(&path) {
                    let restored = self.write_bytes_atomic(&path, &old_bytes, false).is_ok() && lc.bootstrap(&path).is_ok();
                    return Err(e.with_rolled_back(restored));
                }
            }
            return Ok(file_name.to_string());
        }

        // Label 変更: 旧 bootout → 旧削除 → 新書き込み → 新 bootstrap
        bootout_old()?;
        if let Err(e) = self.fs.remove_file(&path) {
            if was_loaded {
                let _ = lc.bootstrap(&path);
            }
            return Err(e.into());
        }
        let result = (|| -> Result<(), AppError> {
            self.write_plist_atomic(&new_path, &new_dict)?;
            if was_enabled {
                lc.enable(&input.label)?;
                lc.bootstrap(&new_path)?;
            } else if was_disabled {
                lc.disable(&input.label)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            if self.fs.exists(&new_path) {
                let _ = self.fs.remove_file(&new_path);
            }
            let restored = self.write_bytes_atomic(&path, &old_bytes, false).is_ok()
                && (!was_loaded || lc.bootstrap(&path).is_ok());
            return Err(e.with_rolled_back(restored));
        }
        Ok(format!("{}.plist", input.label))
    }

    /// 削除: bootout → plist 削除。ログと実行記録は残す。
    pub fn delete_job(&self, file_name: &str) -> Result<(), AppError> {
        let (path, _, _, label) = self.load_managed(file_name)?;
        let lc = self.lc();
        let outcome = match lc.print(&label)? {
            PrintResult::NotLoaded => BootoutOutcome::NotLoaded,
            _ => lc.bootout(&label)?,
        };
        if let Err(e) = self.fs.remove_file(&path) {
            if outcome == BootoutOutcome::Done {
                let _ = lc.bootstrap(&path);
            }
            return Err(e.into());
        }
        Ok(())
    }

    /// 有効化 = enable →（未登録なら）bootstrap、無効化 = bootout → disable。管理外のジョブも可。
    pub fn set_enabled(&self, file_name: &str, enabled: bool) -> Result<(), AppError> {
        let path = self.resolve_file(file_name)?;
        let label = self.load_job(&path)?.label.ok_or_else(|| plist_err(MSG_NO_LABEL))?;
        let lc = self.lc();
        if enabled {
            lc.enable(&label)?;
            match lc.print(&label)? {
                PrintResult::Loaded(_) => Ok(()),
                PrintResult::NotLoaded => lc.bootstrap(&path),
                PrintResult::Failed { code, stderr } => {
                    Err(AppError::Launchctl { operation: "print".into(), exit_code: code, stderr, rolled_back: false })
                }
            }
        } else {
            lc.bootout(&label)?;
            lc.disable(&label)
        }
    }

    pub fn run_now(&self, file_name: &str) -> Result<(), AppError> {
        let path = self.resolve_file(file_name)?;
        let label = self.load_job(&path)?.label.ok_or_else(|| plist_err(MSG_NO_LABEL))?;
        self.lc().kickstart(&label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fakes::{FakeFs, FakeRunner, FixedClock, Journal};
    use crate::domain::schedule::ScheduleInput;

    fn cfg() -> Config {
        Config::from_home(Path::new("/home/u"), 501)
    }

    fn input(label: &str) -> JobInput {
        JobInput { label: label.into(), command: "echo hi".into(), schedule: ScheduleInput::Daily { hour: 9, minute: 0 } }
    }

    fn s(v: &Value) -> &str {
        v.as_string().unwrap()
    }

    #[test]
    fn build_new_plist() {
        let c = cfg();
        let d = build_managed_plist(None, &input("com.a"), None, &c);
        let keys: Vec<&str> = d.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec!["Label", "ProgramArguments", "StartCalendarInterval", "StandardOutPath", "StandardErrorPath", "EnvironmentVariables"]
        );
        let pa: Vec<&str> = d["ProgramArguments"].as_array().unwrap().iter().map(s).collect();
        assert_eq!(pa, vec![c.wrapper_path.to_str().unwrap(), "com.a", "echo hi"]);
        assert_eq!(s(&d["StandardOutPath"]), "/home/u/Library/Logs/com.a/stdout.log");
        assert_eq!(s(&d["StandardErrorPath"]), "/home/u/Library/Logs/com.a/stderr.log");
        assert_eq!(s(&d["EnvironmentVariables"].as_dictionary().unwrap()["PATH"]), DEFAULT_PATH_ENV);
    }

    #[test]
    fn build_preserves_other_keys_and_removes_program() {
        let c = cfg();
        let mut base = Dictionary::new();
        base.insert("Label".into(), Value::String("com.a".into()));
        base.insert("Program".into(), Value::String("/bin/x".into()));
        base.insert("KeepAlive".into(), Value::Boolean(true));
        base.insert("WorkingDirectory".into(), Value::String("/tmp".into()));
        base.insert("RunAtLoad".into(), Value::Boolean(true));
        let mut env = Dictionary::new();
        env.insert("PATH".into(), Value::String("/custom".into()));
        env.insert("FOO".into(), Value::String("bar".into()));
        base.insert("EnvironmentVariables".into(), Value::Dictionary(env));
        base.insert("StandardOutPath".into(), Value::String("/my/out.log".into()));
        base.insert("StandardErrorPath".into(), Value::String(c.stderr_path("com.a").to_string_lossy().into()));

        let d = build_managed_plist(Some(base), &input("com.b"), Some("com.a"), &c);
        assert!(d.get("Program").is_none());
        assert_eq!(d["KeepAlive"], Value::Boolean(true));
        assert_eq!(s(&d["WorkingDirectory"]), "/tmp");
        assert_eq!(d["RunAtLoad"], Value::Boolean(true));
        let env = d["EnvironmentVariables"].as_dictionary().unwrap();
        assert_eq!(s(&env["PATH"]), "/custom");
        assert_eq!(s(&env["FOO"]), "bar");
        assert_eq!(s(&d["StandardOutPath"]), "/my/out.log");
        assert_eq!(s(&d["StandardErrorPath"]), "/home/u/Library/Logs/com.b/stderr.log");
        assert_eq!(s(&d["Label"]), "com.b");
    }

    #[test]
    fn build_adds_path_to_existing_env() {
        let mut base = Dictionary::new();
        let mut env = Dictionary::new();
        env.insert("FOO".into(), Value::String("bar".into()));
        base.insert("EnvironmentVariables".into(), Value::Dictionary(env));
        let d = build_managed_plist(Some(base), &input("com.a"), Some("com.a"), &cfg());
        let env = d["EnvironmentVariables"].as_dictionary().unwrap();
        assert_eq!(s(&env["PATH"]), DEFAULT_PATH_ENV);
        assert_eq!(s(&env["FOO"]), "bar");
    }

    #[test]
    fn plist_path_for_rejects_traversal() {
        let j = Journal::default();
        let svc = JobService::new(
            Arc::new(FakeFs::new(j.clone())),
            Arc::new(FakeRunner::new(j)),
            Arc::new(FixedClock(1)),
            cfg(),
        );
        assert!(svc.plist_path_for("../evil").is_err());
        assert!(svc.plist_path_for("..").is_err());
        assert_eq!(
            svc.plist_path_for("com.a").unwrap(),
            PathBuf::from("/home/u/Library/LaunchAgents/com.a.plist")
        );
    }
}
