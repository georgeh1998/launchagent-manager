use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, PartialEq)]
pub struct DirEntryInfo {
    pub path: PathBuf,
    pub is_file: bool,
}

pub trait FileSystem: Send + Sync {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, p: &Path, data: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, p: &Path) -> io::Result<()>;
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>>;
    fn exists(&self, p: &Path) -> bool;
    fn create_dir_all(&self, p: &Path) -> io::Result<()>;
    /// ファイルが無ければ Ok(None)。
    fn modified_epoch_secs(&self, p: &Path) -> io::Result<Option<i64>>;
    fn set_mode(&self, p: &Path, mode: u32) -> io::Result<()>;
}

pub struct RealFs;

impl FileSystem for RealFs {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }

    fn write(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        std::fs::write(p, data)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_file(&self, p: &Path) -> io::Result<()> {
        std::fs::remove_file(p)
    }

    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
        let mut v = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
            v.push(DirEntryInfo { path: entry.path(), is_file });
        }
        Ok(v)
    }

    fn exists(&self, p: &Path) -> bool {
        p.exists()
    }

    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        std::fs::create_dir_all(p)
    }

    fn modified_epoch_secs(&self, p: &Path) -> io::Result<Option<i64>> {
        match std::fs::metadata(p) {
            Ok(m) => Ok(m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn set_mode(&self, p: &Path, mode: u32) -> io::Result<()> {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_fs_basic_operations() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        let fs = RealFs;
        fs.write(&a, b"hello").unwrap();
        fs.rename(&a, &b).unwrap();
        assert!(!fs.exists(&a));
        assert_eq!(fs.read(&b).unwrap(), b"hello");
        assert!(fs.modified_epoch_secs(&b).unwrap().is_some());
        fs.set_mode(&b, 0o755).unwrap();
        assert_eq!(std::fs::metadata(&b).unwrap().permissions().mode() & 0o777, 0o755);
        let list = fs.list_dir(dir.path()).unwrap();
        assert_eq!(list, vec![DirEntryInfo { path: b.clone(), is_file: true }]);
        fs.remove_file(&b).unwrap();
        assert_eq!(fs.modified_epoch_secs(&b).unwrap(), None);
    }
}
