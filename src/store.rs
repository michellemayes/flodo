//! Disk persistence.
//!
//! Two rules drive everything here:
//!   1. Never truncate a file in place — write a temp file and rename over it,
//!      so a crash mid-write can't leave a half-written list.
//!   2. Never overwrite a file we failed to read — quarantine it instead, so a
//!      parse bug can't silently eat someone's todos.

use crate::model::Store;
use crate::settings::Settings;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const TODOS_FILE: &str = "todos.json";
pub const SETTINGS_FILE: &str = "settings.json";

/// Where state lives. `FLODO_STATE_DIR` wins so tests and the screenshot
/// harness can never touch real data.
pub fn state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FLODO_STATE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();

    #[cfg(target_os = "macos")]
    {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Flodo");
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return PathBuf::from(appdata).join("Flodo");
            }
        }
        return PathBuf::from(home).join("Flodo");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("flodo");
            }
        }
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("flodo")
    }
}

/// Temp file in the same directory (rename is only atomic within a filesystem),
/// fsync, then rename over the target.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

fn quarantine(path: &Path) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut bad = path.as_os_str().to_owned();
    bad.push(format!(".corrupt-{ts}"));
    let bad = PathBuf::from(bad);
    if let Err(e) = std::fs::rename(path, &bad) {
        eprintln!("flodo: could not quarantine {}: {e}", path.display());
    } else {
        eprintln!(
            "flodo: {} could not be parsed; kept a copy at {}",
            path.display(),
            bad.display()
        );
    }
}

/// Outcome of a load, so the UI can tell you when something went wrong instead
/// of silently starting empty.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadNotice {
    Ok,
    Fresh,
    Quarantined(String),
}

pub fn load_todos() -> (Store, LoadNotice) {
    let path = state_dir().join(TODOS_FILE);
    load_json(&path, "todos")
}

pub fn load_settings() -> (Settings, LoadNotice) {
    let path = state_dir().join(SETTINGS_FILE);
    let (mut s, notice): (Settings, LoadNotice) = load_json(&path, "settings");
    s.clamp();
    (s, notice)
}

fn load_json<T: serde::de::DeserializeOwned + Default>(path: &Path, what: &str) -> (T, LoadNotice) {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (T::default(), LoadNotice::Fresh)
        }
        Err(e) => {
            eprintln!("flodo: could not read {}: {e}", path.display());
            return (T::default(), LoadNotice::Fresh);
        }
    };
    match serde_json::from_str::<T>(&raw) {
        Ok(v) => (v, LoadNotice::Ok),
        Err(e) => {
            eprintln!("flodo: {what} file is not valid JSON: {e}");
            quarantine(path);
            (
                T::default(),
                LoadNotice::Quarantined(format!("Couldn't read your {what}; kept a backup copy.")),
            )
        }
    }
}

pub fn save_todos(store: &Store) {
    let path = state_dir().join(TODOS_FILE);
    match serde_json::to_string_pretty(store) {
        // Serialization failing must not touch the existing file at all.
        Err(e) => eprintln!("flodo: could not serialize todos: {e}"),
        Ok(json) => {
            if let Err(e) = write_atomic(&path, &json) {
                eprintln!("flodo: could not save todos: {e}");
            }
        }
    }
}

pub fn save_settings(settings: &Settings) {
    let path = state_dir().join(SETTINGS_FILE);
    match serde_json::to_string_pretty(settings) {
        Err(e) => eprintln!("flodo: could not serialize settings: {e}"),
        Ok(json) => {
            if let Err(e) = write_atomic(&path, &json) {
                eprintln!("flodo: could not save settings: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Todo;

    /// `state_dir()` reads a process-wide env var, so these tests must not run
    /// concurrently with each other.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!("flodo-test-{tag}-{nanos}"));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn with_state_dir<R>(tag: &str, f: impl FnOnce(&Path) -> R) -> R {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new(tag);
        // SAFETY: serialised by LOCK; these tests are the only env mutators.
        unsafe { std::env::set_var("FLODO_STATE_DIR", &dir.0) };
        let out = f(&dir.0);
        unsafe { std::env::remove_var("FLODO_STATE_DIR") };
        out
    }

    #[test]
    fn state_dir_honours_the_env_override() {
        with_state_dir("envdir", |dir| {
            assert_eq!(state_dir(), dir);
        });
    }

    #[test]
    fn a_missing_file_loads_fresh_rather_than_failing() {
        with_state_dir("missing", |_| {
            let (store, notice) = load_todos();
            assert!(store.todos.is_empty());
            assert_eq!(notice, LoadNotice::Fresh);
        });
    }

    #[test]
    fn todos_round_trip_through_disk() {
        with_state_dir("roundtrip", |_| {
            let mut s = Store::default();
            s.add("write a test");
            s.add("watch it pass");
            let id = s.todos[0].id;
            s.toggle(id);
            s.todos[1].body = "```rust\nfn main() {}\n```".into();
            save_todos(&s);

            let (loaded, notice) = load_todos();
            assert_eq!(notice, LoadNotice::Ok);
            assert_eq!(loaded.todos.len(), 2);
            assert_eq!(loaded.todos[0].id, id);
            assert!(loaded.todos[0].done);
            assert!(loaded.todos[1].body.contains("fn main"));
        });
    }

    #[test]
    fn a_corrupt_file_is_quarantined_with_its_bytes_intact() {
        with_state_dir("corrupt", |dir| {
            let path = dir.join(TODOS_FILE);
            std::fs::write(&path, "{ this is not json").unwrap();

            let (store, notice) = load_todos();
            assert!(store.todos.is_empty());
            assert!(matches!(notice, LoadNotice::Quarantined(_)));
            assert!(!path.exists(), "the bad file must be moved aside");

            let backups: Vec<_> = std::fs::read_dir(dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
                .collect();
            assert_eq!(backups.len(), 1);
            let kept = std::fs::read_to_string(backups[0].path()).unwrap();
            assert_eq!(kept, "{ this is not json", "original bytes must survive");
        });
    }

    #[test]
    fn a_newer_schema_version_is_read_not_discarded() {
        with_state_dir("newer", |dir| {
            std::fs::write(
                dir.join(TODOS_FILE),
                r#"{"version":99,"todos":[{"id":5,"title":"from the future","futureFlag":true}],
                    "futureTop":"keep me"}"#,
            )
            .unwrap();

            let (store, notice) = load_todos();
            assert_eq!(notice, LoadNotice::Ok);
            assert_eq!(store.version, 99);
            assert_eq!(store.todos[0].title, "from the future");

            // And saving it back must not drop the unknown keys.
            save_todos(&store);
            let raw = std::fs::read_to_string(dir.join(TODOS_FILE)).unwrap();
            assert!(raw.contains("futureTop"));
            assert!(raw.contains("futureFlag"));
        });
    }

    #[test]
    fn settings_round_trip_and_clamp_on_load() {
        with_state_dir("settings", |dir| {
            std::fs::write(dir.join(SETTINGS_FILE), r#"{"font_size": 999.0}"#).unwrap();
            let (s, notice) = load_settings();
            assert_eq!(notice, LoadNotice::Ok);
            assert_eq!(s.font_size, crate::settings::FONT_SIZE_RANGE.1);

            save_settings(&s);
            let (again, _) = load_settings();
            assert_eq!(again.font_size, s.font_size);
        });
    }

    #[test]
    fn saving_leaves_no_temp_file_behind() {
        with_state_dir("notmp", |dir| {
            let mut s = Store::default();
            s.add("x");
            save_todos(&s);
            let leftovers: Vec<_> = std::fs::read_dir(dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
                .collect();
            assert!(leftovers.is_empty());
        });
    }

    #[test]
    fn a_save_replaces_the_previous_contents_completely() {
        with_state_dir("replace", |_| {
            let mut s = Store::default();
            for i in 0..10 {
                s.add(format!("task {i}"));
            }
            save_todos(&s);

            let small = Store {
                todos: vec![Todo::new(1, "only one")],
                ..Default::default()
            };
            save_todos(&small);

            let (loaded, _) = load_todos();
            assert_eq!(loaded.todos.len(), 1);
            assert_eq!(loaded.todos[0].title, "only one");
        });
    }
}
