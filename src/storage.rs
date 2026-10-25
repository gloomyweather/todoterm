//! Picking the data file, loading it safely and saving it atomically.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};

use crate::task::Task;

pub const FILE_NAME: &str = "todo.json";

/// Where the task list lives, in order of precedence:
/// `--file`, `$TODOTERM_FILE`, `file` from config, `./todo.json` if it exists,
/// then `$XDG_DATA_HOME/todoterm/todo.json`.
pub fn resolve_path(
    cli: Option<PathBuf>,
    env: Option<OsString>,
    config: Option<PathBuf>,
    cwd: &Path,
    data_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    let explicit = cli
        .or_else(|| env.filter(|v| !v.is_empty()).map(PathBuf::from))
        .or(config);
    if let Some(path) = explicit {
        let path = expand_tilde(&path);
        return Ok(if path.is_relative() { cwd.join(path) } else { path });
    }
    let local = cwd.join(FILE_NAME);
    if local.is_file() {
        return Ok(local);
    }
    let data_dir = data_dir.ok_or_else(|| anyhow!("could not find a data directory, use --file"))?;
    Ok(data_dir.join("todoterm").join(FILE_NAME))
}

pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

/// Modification time and size, used to tell our own writes from outside ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: SystemTime,
    len: u64,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok()?;
    Some(Stamp {
        modified: meta.modified().ok()?,
        len: meta.len(),
    })
}

pub struct Storage {
    path: PathBuf,
    seen: Option<Stamp>,
}

impl Storage {
    pub fn new(path: PathBuf) -> Self {
        Self { path, seen: None }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load tasks. A missing or empty file is an empty list. A file that does
    /// not parse is copied to `todo.json.corrupt-<timestamp>` and reported as
    /// an error; it is never overwritten here.
    pub fn load(&mut self) -> Result<Vec<Task>> {
        let current = stamp(&self.path);
        let data = match fs::read_to_string(&self.path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.seen = None;
                return Ok(Vec::new());
            }
            Err(e) => return Err(e).with_context(|| format!("could not read {}", self.path.display())),
        };
        // Remember the file even if it is broken, so it is not re-read every tick.
        self.seen = current;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&data).map_err(|err| {
            let backup = self.backup_corrupt(&data);
            let backup = match backup {
                Ok(p) => format!("a copy was saved to {}", p.display()),
                Err(e) => format!("could not save a copy: {e:#}"),
            };
            anyhow!(
                "{} is not valid ({err}); {backup}. fix or move the file, todoterm will not overwrite it",
                self.path.display()
            )
        })
    }

    fn backup_corrupt(&self, data: &str) -> Result<PathBuf> {
        let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".corrupt-{ts}"));
        let backup = self.path.with_file_name(name);
        if !backup.exists() {
            fs::write(&backup, data)?;
        }
        Ok(backup)
    }

    /// Write to a temp file next to the target, fsync, then rename over it.
    pub fn save(&mut self, tasks: &[Task]) -> Result<()> {
        // Follow a symlinked todo.json so the link itself is not replaced.
        let target = fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        let dir = match target.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;

        let mut json = serde_json::to_string_pretty(tasks)?;
        json.push('\n');

        let mut tmp_name = OsString::from(".");
        tmp_name.push(target.file_name().unwrap_or_else(|| FILE_NAME.as_ref()));
        tmp_name.push(format!(".tmp-{}", std::process::id()));
        let tmp = dir.join(tmp_name);

        let write = || -> std::io::Result<()> {
            let mut f = File::create(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
            fs::rename(&tmp, &target)?;
            // Make the rename itself durable.
            if let Ok(d) = File::open(&dir) {
                let _ = d.sync_all();
            }
            Ok(())
        };
        if let Err(e) = write() {
            let _ = fs::remove_file(&tmp);
            return Err(e).with_context(|| format!("could not save {}", self.path.display()));
        }
        self.seen = stamp(&self.path);
        Ok(())
    }

    /// True when the file changed on disk since we last read or wrote it.
    pub fn changed_externally(&self) -> bool {
        stamp(&self.path) != self.seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{parse_input, Task};

    fn task(id: u32, text: &str) -> Task {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Task::new(id, parse_input(text, today))
    }

    #[test]
    fn path_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        let data = Some(PathBuf::from("/data"));
        let cli = Some(PathBuf::from("/cli.json"));
        let env = Some(OsString::from("/env.json"));
        let cfg = Some(PathBuf::from("/cfg.json"));

        let r = |cli: &Option<PathBuf>, env: &Option<OsString>, cfg: &Option<PathBuf>| {
            resolve_path(cli.clone(), env.clone(), cfg.clone(), cwd, data.clone()).unwrap()
        };
        assert_eq!(r(&cli, &env, &cfg), PathBuf::from("/cli.json"));
        assert_eq!(r(&None, &env, &cfg), PathBuf::from("/env.json"));
        assert_eq!(r(&None, &Some(OsString::new()), &cfg), PathBuf::from("/cfg.json"));
        assert_eq!(r(&None, &None, &cfg), PathBuf::from("/cfg.json"));
        assert_eq!(r(&None, &None, &None), PathBuf::from("/data/todoterm/todo.json"));

        fs::write(cwd.join("todo.json"), "[]").unwrap();
        assert_eq!(r(&None, &None, &None), cwd.join("todo.json"));
        // Explicit choices still win over ./todo.json.
        assert_eq!(r(&None, &None, &cfg), PathBuf::from("/cfg.json"));
        // Relative paths are relative to the working directory.
        let rel = Some(PathBuf::from("lists/a.json"));
        assert_eq!(r(&rel, &None, &None), cwd.join("lists/a.json"));
    }

    #[test]
    fn save_creates_dirs_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/er/todo.json");
        let mut s = Storage::new(path.clone());
        assert!(s.load().unwrap().is_empty());
        let tasks = vec![task(1, "a !! @2026-02-01"), task(2, "b")];
        s.save(&tasks).unwrap();
        assert!(!s.changed_externally());
        let mut s2 = Storage::new(path.clone());
        assert_eq!(s2.load().unwrap(), tasks);
        // No temp files left behind.
        let names: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn loads_old_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo.json");
        fs::write(&path, r#"[{"id": 3, "description": "legacy", "done": false}]"#).unwrap();
        let tasks = Storage::new(path).load().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].description, "legacy");
        assert_eq!(tasks[0].id, 3);
    }

    #[test]
    fn empty_file_is_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo.json");
        fs::write(&path, "  \n").unwrap();
        assert!(Storage::new(path).load().unwrap().is_empty());
    }

    #[test]
    fn corrupt_file_is_kept_and_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo.json");
        let garbage = r#"[{"id": 1, "description": "half"#;
        fs::write(&path, garbage).unwrap();
        let err = Storage::new(path.clone()).load().unwrap_err().to_string();
        assert!(err.contains("todo.json"), "{err}");
        assert!(err.contains("corrupt-"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), garbage);
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("todo.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), garbage);
    }

    #[test]
    fn detects_external_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo.json");
        let mut s = Storage::new(path.clone());
        s.save(&[task(1, "a")]).unwrap();
        assert!(!s.changed_externally());
        fs::write(&path, "[]\n\n").unwrap();
        assert!(s.changed_externally());
        assert!(s.load().unwrap().is_empty());
        assert!(!s.changed_externally());
        fs::remove_file(&path).unwrap();
        assert!(s.changed_externally());
    }

    #[cfg(unix)]
    #[test]
    fn save_follows_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.json");
        let link = dir.path().join("todo.json");
        fs::write(&real, "[]").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut s = Storage::new(link.clone());
        s.save(&[task(1, "x")]).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(fs::read_to_string(&real).unwrap().contains("\"x\""));
    }
}
