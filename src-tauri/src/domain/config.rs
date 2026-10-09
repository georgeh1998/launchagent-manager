use std::path::{Path, PathBuf};

use super::error::AppError;

pub const APP_DIR_NAME: &str = "launchagent-manager";
pub const DEFAULT_PATH_ENV: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";
pub const LAUNCHCTL: &str = "/bin/launchctl";
pub const PLUTIL: &str = "/usr/bin/plutil";

/// パス・uid などの環境依存値。テストでは一時ディレクトリを注入する。
#[derive(Clone, Debug)]
pub struct Config {
    pub uid: u32,
    pub home: PathBuf,
    pub launch_agents_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub app_support_dir: PathBuf,
    pub wrapper_path: PathBuf,
    pub runs_dir: PathBuf,
}

impl Config {
    pub fn from_home(home: &Path, uid: u32) -> Self {
        let library = home.join("Library");
        let app_support_dir = library.join("Application Support").join(APP_DIR_NAME);
        Config {
            uid,
            home: home.to_path_buf(),
            launch_agents_dir: library.join("LaunchAgents"),
            logs_dir: library.join("Logs"),
            wrapper_path: app_support_dir.join("bin").join("run-job.sh"),
            runs_dir: app_support_dir.join("runs"),
            app_support_dir,
        }
    }

    pub fn from_env() -> Result<Self, AppError> {
        let home = std::env::var_os("HOME").ok_or_else(|| AppError::Io {
            message: "HOME が設定されていません".into(),
        })?;
        // SAFETY: getuid は常に成功し副作用もない
        let uid = unsafe { libc::getuid() };
        Ok(Self::from_home(Path::new(&home), uid))
    }

    pub fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    pub fn service_target(&self, label: &str) -> String {
        format!("gui/{}/{}", self.uid, label)
    }

    pub fn job_log_dir(&self, label: &str) -> PathBuf {
        self.logs_dir.join(label)
    }

    pub fn stdout_path(&self, label: &str) -> PathBuf {
        self.job_log_dir(label).join("stdout.log")
    }

    pub fn stderr_path(&self, label: &str) -> PathBuf {
        self.job_log_dir(label).join("stderr.log")
    }

    pub fn run_record_path(&self, label: &str) -> PathBuf {
        self.runs_dir.join(format!("{label}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_home_builds_paths() {
        let c = Config::from_home(Path::new("/tmp/h"), 501);
        assert_eq!(c.launch_agents_dir, PathBuf::from("/tmp/h/Library/LaunchAgents"));
        assert_eq!(c.logs_dir, PathBuf::from("/tmp/h/Library/Logs"));
        assert_eq!(
            c.app_support_dir,
            PathBuf::from("/tmp/h/Library/Application Support/launchagent-manager")
        );
        assert_eq!(
            c.wrapper_path,
            PathBuf::from("/tmp/h/Library/Application Support/launchagent-manager/bin/run-job.sh")
        );
        assert_eq!(
            c.run_record_path("a.b"),
            PathBuf::from("/tmp/h/Library/Application Support/launchagent-manager/runs/a.b.json")
        );
    }

    #[test]
    fn service_target_and_domain() {
        let c = Config::from_home(Path::new("/tmp/h"), 501);
        assert_eq!(c.domain(), "gui/501");
        assert_eq!(c.service_target("x"), "gui/501/x");
    }

    #[test]
    fn log_paths_differ_per_job() {
        let c = Config::from_home(Path::new("/tmp/h"), 501);
        assert_eq!(c.stdout_path("a"), PathBuf::from("/tmp/h/Library/Logs/a/stdout.log"));
        assert_eq!(c.stderr_path("a"), PathBuf::from("/tmp/h/Library/Logs/a/stderr.log"));
        assert_ne!(c.stdout_path("a"), c.stdout_path("b"));
    }
}
