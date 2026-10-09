use std::io;
use std::process::Command;

/// 外部コマンドの実行結果。シグナル終了などで終了コードが取れない場合は -1。
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CmdOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn ok(stdout: &str) -> Self {
        CmdOutput { code: 0, stdout: stdout.into(), stderr: String::new() }
    }

    pub fn fail(code: i32, stderr: &str) -> Self {
        CmdOutput { code, stdout: String::new(), stderr: stderr.into() }
    }
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[String]) -> io::Result<CmdOutput>;
}

pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, program: &str, args: &[String]) -> io::Result<CmdOutput> {
        let out = Command::new(program).args(args).output()?;
        Ok(CmdOutput {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_runner_captures_output_and_code() {
        let out = RealRunner.run("/bin/echo", &["hi".into()]).unwrap();
        assert_eq!(out.code, 0);
        assert_eq!(out.stdout, "hi\n");
        let out = RealRunner.run("/usr/bin/false", &[]).unwrap();
        assert_eq!(out.code, 1);
    }
}
