//! テスト用のフェイク実装。結合テスト（tests/）からも使えるよう公開している。

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::clock::Clock;
use super::cmd::{CmdOutput, CommandRunner};
use super::config::LAUNCHCTL;
use super::fs::{DirEntryInfo, FileSystem};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Cmd { program: String, args: Vec<String> },
    Write(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
    Remove(PathBuf),
    Mkdir(PathBuf),
    SetMode(PathBuf, u32),
}

/// 順序検証用に要約した操作。plutil・一時ファイル・mkdir は含めない。
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Launchctl(Vec<String>),
    WriteFile(PathBuf),
    Remove(PathBuf),
}

/// FakeFs と FakeRunner が共有する操作ログ。
#[derive(Clone, Default)]
pub struct Journal(Arc<Mutex<Vec<Event>>>);

fn is_tmp(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.') && n.contains(".tmp-"))
        .unwrap_or(false)
}

impl Journal {
    pub fn push(&self, e: Event) {
        self.0.lock().unwrap().push(e);
    }

    pub fn events(&self) -> Vec<Event> {
        self.0.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    pub fn ops(&self) -> Vec<Op> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Cmd { program, args } if program == LAUNCHCTL => Some(Op::Launchctl(args)),
                Event::Rename { to, .. } => Some(Op::WriteFile(to)),
                Event::Remove(p) if !is_tmp(&p) => Some(Op::Remove(p)),
                _ => None,
            })
            .collect()
    }

    /// 状態を変える操作だけ（launchctl の print / print-disabled / list を除く）。
    pub fn mutating_ops(&self) -> Vec<Op> {
        self.ops()
            .into_iter()
            .filter(|o| match o {
                Op::Launchctl(a) => !matches!(a.first().map(String::as_str), Some("print" | "print-disabled" | "list")),
                _ => true,
            })
            .collect()
    }

    /// launchctl の引数だけを並べたもの。
    pub fn launchctl_ops(&self) -> Vec<Vec<String>> {
        self.ops()
            .into_iter()
            .filter_map(|o| match o {
                Op::Launchctl(a) => Some(a),
                _ => None,
            })
            .collect()
    }
}

type PathPred = Box<dyn Fn(&Path) -> bool + Send + Sync>;

#[derive(Default)]
struct FsState {
    files: BTreeMap<PathBuf, (Vec<u8>, i64)>,
    dirs: BTreeSet<PathBuf>,
    modes: BTreeMap<PathBuf, u32>,
}

pub struct FakeFs {
    journal: Journal,
    state: Mutex<FsState>,
    fail_write: Mutex<Option<PathPred>>,
    fail_rename_to: Mutex<Option<PathBuf>>,
    fail_remove: Mutex<Option<PathBuf>>,
}

impl FakeFs {
    pub fn new(journal: Journal) -> Self {
        FakeFs {
            journal,
            state: Mutex::new(FsState::default()),
            fail_write: Mutex::new(None),
            fail_rename_to: Mutex::new(None),
            fail_remove: Mutex::new(None),
        }
    }

    pub fn with_file(self, path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> Self {
        self.put(path, bytes);
        self
    }

    pub fn with_dir(self, path: impl AsRef<Path>) -> Self {
        self.state.lock().unwrap().dirs.insert(path.as_ref().to_path_buf());
        self
    }

    pub fn with_mtime(self, path: impl AsRef<Path>, mtime: i64) -> Self {
        if let Some(f) = self.state.lock().unwrap().files.get_mut(path.as_ref()) {
            f.1 = mtime;
        }
        self
    }

    /// ジャーナルに記録せずにファイルを置く。
    pub fn put(&self, path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        self.state
            .lock()
            .unwrap()
            .files
            .insert(path.as_ref().to_path_buf(), (bytes.as_ref().to_vec(), 0));
    }

    pub fn fail_write_on(&self, pred: impl Fn(&Path) -> bool + Send + Sync + 'static) {
        *self.fail_write.lock().unwrap() = Some(Box::new(pred));
    }

    pub fn fail_rename_to(&self, path: impl AsRef<Path>) {
        *self.fail_rename_to.lock().unwrap() = Some(path.as_ref().to_path_buf());
    }

    pub fn fail_remove(&self, path: impl AsRef<Path>) {
        *self.fail_remove.lock().unwrap() = Some(path.as_ref().to_path_buf());
    }

    pub fn clear_failures(&self) {
        *self.fail_write.lock().unwrap() = None;
        *self.fail_rename_to.lock().unwrap() = None;
        *self.fail_remove.lock().unwrap() = None;
    }

    pub fn get(&self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        self.state.lock().unwrap().files.get(path.as_ref()).map(|f| f.0.clone())
    }

    pub fn has_dir(&self, path: impl AsRef<Path>) -> bool {
        self.state.lock().unwrap().dirs.contains(path.as_ref())
    }

    pub fn mode(&self, path: impl AsRef<Path>) -> Option<u32> {
        self.state.lock().unwrap().modes.get(path.as_ref()).copied()
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.state.lock().unwrap().files.keys().cloned().collect()
    }
}

fn injected() -> io::Error {
    io::Error::new(io::ErrorKind::Other, "injected failure")
}

impl FileSystem for FakeFs {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        self.get(p).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
    }

    fn write(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(pred) = self.fail_write.lock().unwrap().as_ref() {
            if pred(p) {
                return Err(injected());
            }
        }
        self.journal.push(Event::Write(p.to_path_buf()));
        self.put(p, data);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fail_rename_to.lock().unwrap().as_deref() == Some(to) {
            return Err(injected());
        }
        let mut st = self.state.lock().unwrap();
        let f = st.files.remove(from).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))?;
        st.files.insert(to.to_path_buf(), f);
        drop(st);
        self.journal.push(Event::Rename { from: from.to_path_buf(), to: to.to_path_buf() });
        Ok(())
    }

    fn remove_file(&self, p: &Path) -> io::Result<()> {
        if self.fail_remove.lock().unwrap().as_deref() == Some(p) {
            return Err(injected());
        }
        let removed = self.state.lock().unwrap().files.remove(p);
        if removed.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
        }
        self.journal.push(Event::Remove(p.to_path_buf()));
        Ok(())
    }

    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
        let st = self.state.lock().unwrap();
        let mut v: Vec<DirEntryInfo> = st
            .files
            .keys()
            .filter(|p| p.parent() == Some(dir))
            .map(|p| DirEntryInfo { path: p.clone(), is_file: true })
            .collect();
        v.extend(
            st.dirs
                .iter()
                .filter(|p| p.parent() == Some(dir))
                .map(|p| DirEntryInfo { path: p.clone(), is_file: false }),
        );
        Ok(v)
    }

    fn exists(&self, p: &Path) -> bool {
        let st = self.state.lock().unwrap();
        st.files.contains_key(p) || st.dirs.contains(p)
    }

    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        self.journal.push(Event::Mkdir(p.to_path_buf()));
        self.state.lock().unwrap().dirs.insert(p.to_path_buf());
        Ok(())
    }

    fn modified_epoch_secs(&self, p: &Path) -> io::Result<Option<i64>> {
        Ok(self.state.lock().unwrap().files.get(p).map(|f| f.1))
    }

    fn set_mode(&self, p: &Path, mode: u32) -> io::Result<()> {
        self.journal.push(Event::SetMode(p.to_path_buf(), mode));
        self.state.lock().unwrap().modes.insert(p.to_path_buf(), mode);
        Ok(())
    }
}

struct Response {
    prefix: Vec<String>,
    output: CmdOutput,
    remaining: Option<usize>,
}

/// 引数の先頭一致でレスポンスを返すフェイク。後に登録したものが優先。既定は終了コード 0。
pub struct FakeRunner {
    journal: Journal,
    responses: Mutex<Vec<Response>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl FakeRunner {
    pub fn new(journal: Journal) -> Self {
        FakeRunner { journal, responses: Mutex::new(Vec::new()), calls: Mutex::new(Vec::new()) }
    }

    pub fn respond(&self, args_prefix: &[&str], output: CmdOutput) {
        self.push(args_prefix, output, None);
    }

    /// 指定回数だけ使われるレスポンス。
    pub fn respond_times(&self, args_prefix: &[&str], output: CmdOutput, times: usize) {
        self.push(args_prefix, output, Some(times));
    }

    fn push(&self, args_prefix: &[&str], output: CmdOutput, remaining: Option<usize>) {
        self.responses.lock().unwrap().push(Response {
            prefix: args_prefix.iter().map(|s| s.to_string()).collect(),
            output,
            remaining,
        });
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().iter().map(|(_, a)| a.clone()).collect()
    }

    pub fn calls_with_program(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[String]) -> io::Result<CmdOutput> {
        self.calls.lock().unwrap().push((program.to_string(), args.to_vec()));
        self.journal.push(Event::Cmd { program: program.to_string(), args: args.to_vec() });
        let mut responses = self.responses.lock().unwrap();
        for r in responses.iter_mut().rev() {
            if r.remaining == Some(0) || !args.starts_with(&r.prefix) {
                continue;
            }
            if let Some(n) = r.remaining.as_mut() {
                *n -= 1;
            }
            return Ok(r.output.clone());
        }
        Ok(CmdOutput::default())
    }
}

pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_millis(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_fs_failure_injection() {
        let j = Journal::default();
        let fs = FakeFs::new(j.clone()).with_file("/a", "x");
        fs.fail_write_on(|p| p == Path::new("/b"));
        assert!(fs.write(Path::new("/b"), b"y").is_err());
        fs.fail_rename_to("/c");
        assert!(fs.rename(Path::new("/a"), Path::new("/c")).is_err());
        fs.fail_remove("/a");
        assert!(fs.remove_file(Path::new("/a")).is_err());
        assert_eq!(fs.get("/a").unwrap(), b"x");
        assert!(j.events().is_empty());
    }

    #[test]
    fn fake_runner_prefix_and_times() {
        let r = FakeRunner::new(Journal::default());
        r.respond(&["print"], CmdOutput::fail(113, "x"));
        r.respond_times(&["print"], CmdOutput::ok("once"), 1);
        assert_eq!(r.run("p", &["print".into(), "a".into()]).unwrap().stdout, "once");
        assert_eq!(r.run("p", &["print".into()]).unwrap().code, 113);
        assert_eq!(r.run("p", &["list".into()]).unwrap().code, 0);
    }
}
