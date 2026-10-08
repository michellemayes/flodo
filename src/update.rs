//! Finding out about a newer release, and installing it in place.
//!
//! Every release publishes a small `latest.json` next to its archives (see
//! `.github/workflows/release.yml`). GitHub's `releases/latest/download/` path
//! always serves the newest one, without touching the rate-limited API.
//!
//! The network goes through the system `curl` rather than an HTTP and TLS
//! stack compiled in: it ships with macOS, Windows 10 and later, and every
//! desktop Linux, and it keeps the binary the size it is. Archives are checked
//! against the SHA-256 in the manifest before anything on disk is touched.
//!
//! Installing swaps the new build in beside the old one with renames, so a
//! failure at any step leaves the running copy exactly where it was.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;

const MANIFEST_URL: &str =
    "https://github.com/michellemayes/flodo/releases/latest/download/latest.json";
/// How often a long-running Flodo looks again. It is the kind of app that
/// stays open for weeks.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Which archive in the manifest is this build's.
const PLATFORM: Option<&str> = if cfg!(target_os = "macos") {
    Some("macos-universal")
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x86_64")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("windows-x86_64")
} else {
    None
};

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    version: String,
    /// The release page, for anyone who would rather download it themselves.
    url: String,
    #[serde(default)]
    assets: HashMap<String, Asset>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Asset {
    url: String,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    asset: Option<Asset>,
    /// False when there is no build for this platform, or this copy is not
    /// somewhere it can replace itself (a bare `cargo run`, a translocated
    /// app). The page can still be opened.
    pub installable: bool,
}

/// `1.2.3` or `v1.2.3`. Anything else is not something this compares.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.trim().trim_start_matches('v').split('.');
    let v = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(v)
}

fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn release_from(manifest: Manifest, current: &str) -> Option<Release> {
    if !is_newer(&manifest.version, current) {
        return None;
    }
    let asset = PLATFORM.and_then(|p| manifest.assets.get(p).cloned());
    let installable = asset.is_some() && install_target().is_ok();
    Some(Release {
        version: manifest.version.trim_start_matches('v').to_string(),
        page: manifest.url,
        asset,
        installable,
    })
}

/// A command with no console window. A GUI-subsystem Windows build would
/// otherwise flash one up for every `curl` it runs.
fn quiet(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn run(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("couldn't run {what}: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim().lines().last().unwrap_or("").trim();
        Err(if err.is_empty() {
            format!("{what} failed")
        } else {
            format!("{what} failed: {err}")
        })
    }
}

/// Windows has had both in System32 since 10 1803. Named in full, because a
/// Git for Windows `tar` earlier on the PATH can't read a zip.
fn system_tool(name: &str) -> String {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        format!(r"{root}\System32\{name}.exe")
    } else {
        name.to_string()
    }
}

fn curl(url: &str, max_time: u64) -> Command {
    let mut cmd = quiet(&system_tool("curl"));
    cmd.args(["--fail", "--silent", "--show-error", "--location"])
        // `file` is only there so FLODO_UPDATE_URL can point at a local test
        // manifest; redirects may only ever go to https.
        .args(["--proto", "=https,file", "--proto-redir", "=https"])
        .args(["--connect-timeout", "15", "--max-time"])
        .arg(max_time.to_string())
        .arg(url);
    cmd
}

/// Asks for the latest manifest. `Ok(None)` means this is already the newest.
pub fn check() -> Result<Option<Release>, String> {
    let url = std::env::var("FLODO_UPDATE_URL").unwrap_or_else(|_| MANIFEST_URL.to_string());
    let body = run(&mut curl(&url, 30), "the update check")?;
    let manifest: Manifest = serde_json::from_slice(&body)
        .map_err(|e| format!("the update manifest didn't make sense: {e}"))?;
    Ok(release_from(manifest, current_version()))
}

/// What gets replaced: the whole `.app` on macOS, the executable elsewhere.
fn install_target() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("couldn't find this copy of Flodo: {e}"))?;

    if cfg!(target_os = "macos") {
        // …/Flodo.app/Contents/MacOS/flodo
        let app = exe
            .ancestors()
            .nth(3)
            .filter(|a| a.extension().is_some_and(|e| e == "app"))
            .ok_or("this copy isn't running from Flodo.app")?;
        // Gatekeeper runs a quarantined app from a read-only copy somewhere
        // random; replacing that would change nothing anyone launches.
        if app.to_string_lossy().contains("/AppTranslocation/") {
            return Err("move Flodo.app to Applications first".into());
        }
        Ok(app.to_path_buf())
    } else {
        Ok(exe)
    }
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// A scratch directory beside the install, so every move out of it is a
/// rename on one filesystem. Gone again however the install ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn extract(archive: &Path, into: &Path) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") {
        // ditto, not unzip: it keeps the bundle's symlinks and signature intact.
        let mut c = quiet("ditto");
        c.args(["-x", "-k"]).arg(archive).arg(into);
        c
    } else {
        // bsdtar on Windows reads zips; GNU tar on Linux reads the .tar.gz.
        let mut c = quiet(&system_tool("tar"));
        c.arg("-xf").arg(archive).arg("-C").arg(into);
        c
    };
    run(&mut cmd, "unpacking the update").map(|_| ())
}

/// Downloads, verifies and swaps in `release`. Returns what to relaunch.
pub fn install(release: &Release) -> Result<PathBuf, String> {
    let asset = release
        .asset
        .as_ref()
        .ok_or("there's no build for this platform")?;
    let target = install_target()?;
    let parent = target
        .parent()
        .ok_or("this copy of Flodo has nowhere to go")?;

    let scratch = Scratch(parent.join(format!(".flodo-update-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::create_dir(&scratch.0)
        .map_err(|e| format!("can't write next to {} ({e})", target.display()))?;

    let archive = scratch.0.join("download");
    run(
        curl(&asset.url, 600).arg("--output").arg(&archive),
        "the download",
    )?;
    let got = sha256_hex(&archive)?;
    if !got.eq_ignore_ascii_case(asset.sha256.trim()) {
        return Err("the download didn't match its checksum".into());
    }

    let unpacked = scratch.0.join("new");
    std::fs::create_dir(&unpacked).map_err(|e| e.to_string())?;
    extract(&archive, &unpacked)?;

    let name = if cfg!(target_os = "macos") {
        "Flodo.app"
    } else if cfg!(windows) {
        "flodo.exe"
    } else {
        "flodo"
    };
    let fresh = unpacked.join(name);
    if !fresh.exists() {
        return Err(format!("the update had no {name} in it"));
    }
    swap(&fresh, &target, &scratch.0)?;
    Ok(target)
}

#[cfg(target_os = "macos")]
fn swap(fresh: &Path, target: &Path, scratch: &Path) -> Result<(), String> {
    // Directories can't be renamed over each other, so the old bundle steps
    // aside first, and comes back if the new one can't take its place. The
    // running process doesn't mind its bundle moving; it is already loaded.
    let old = scratch.join("old.app");
    std::fs::rename(target, &old).map_err(|e| format!("couldn't replace Flodo.app: {e}"))?;
    if let Err(e) = std::fs::rename(fresh, target) {
        let _ = std::fs::rename(&old, target);
        return Err(format!("couldn't replace Flodo.app: {e}"));
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn swap(fresh: &Path, target: &Path, _scratch: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(fresh, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    // Renaming over a running executable is fine here: the old inode lives on
    // until this process exits.
    std::fs::rename(fresh, target).map_err(|e| format!("couldn't replace flodo: {e}"))
}

#[cfg(windows)]
fn swap(fresh: &Path, target: &Path, _scratch: &Path) -> Result<(), String> {
    // A running .exe can't be overwritten or deleted, but it can be renamed.
    // `tidy` removes the old one on the next launch.
    let old = old_exe(target);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(target, &old).map_err(|e| format!("couldn't replace flodo.exe: {e}"))?;
    if let Err(e) = std::fs::rename(fresh, target) {
        let _ = std::fs::rename(&old, target);
        return Err(format!("couldn't replace flodo.exe: {e}"));
    }
    Ok(())
}

#[cfg(windows)]
fn old_exe(target: &Path) -> PathBuf {
    target.with_extension("old.exe")
}

/// Clears up after the last update. Only Windows leaves anything behind.
pub fn tidy() {
    #[cfg(windows)]
    if let Ok(target) = install_target() {
        let _ = std::fs::remove_file(old_exe(&target));
    }
}

/// Starts the new copy once this one has exited, so the two never write the
/// same files at once. The caller closes the window straight after.
pub fn relaunch(target: &Path) -> Result<(), String> {
    let pid = std::process::id().to_string();

    #[cfg(unix)]
    let mut cmd = {
        use std::os::unix::process::CommandExt;
        let mut c = quiet("/bin/sh");
        c.arg("-c")
            .arg(
                r#"pid=$1; shift; while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done; exec "$@""#,
            )
            .arg("sh")
            .arg(&pid);
        if cfg!(target_os = "macos") {
            c.arg("/usr/bin/open");
        }
        c.arg(target).process_group(0);
        c
    };

    #[cfg(windows)]
    let mut cmd = {
        let path = target.to_string_lossy().replace('\'', "''");
        let mut c = quiet("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command"])
            .arg(format!(
                "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Start-Process -FilePath '{path}'"
            ));
        c
    };

    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't restart Flodo: {e}"))
}

// ------------------------------------------------------------- the app's view

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(Release),
    /// Installed; waiting to be relaunched into.
    Installed(PathBuf),
    Failed(String),
}

enum Done {
    Checked(Result<Option<Release>, String>),
    Installed(Result<PathBuf, String>),
}

/// Runs checks and installs off the UI thread and remembers how they went.
pub struct Updater {
    pub status: Status,
    rx: Option<Receiver<Done>>,
    /// A check someone asked for reports failures; the daily one doesn't.
    manual: bool,
    last_check: Option<Instant>,
    /// The version whose banner was closed. It stays closed for this session.
    pub dismissed: Option<String>,
    /// The release the last failed install was for, so its page can be offered.
    pub failed_release: Option<Release>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            rx: None,
            manual: false,
            last_check: None,
            dismissed: None,
            failed_release: None,
        }
    }
}

impl Updater {
    fn busy(&self) -> bool {
        self.rx.is_some()
    }

    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.busy() {
            return;
        }
        self.manual = manual;
        self.last_check = Some(Instant::now());
        if manual {
            self.status = Status::Checking;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Done::Checked(check()));
            ctx.request_repaint();
        });
    }

    pub fn install(&mut self, ctx: &egui::Context) {
        let release = match &self.status {
            Status::Available(r) if !self.busy() => r.clone(),
            _ => return,
        };
        self.status = Status::Installing(release.clone());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Done::Installed(install(&release)));
            ctx.request_repaint();
        });
    }

    /// Collects finished work, and starts the daily check when it is due.
    pub fn poll(&mut self, ctx: &egui::Context, automatic: bool) {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(done) => {
                    self.rx = None;
                    self.finish(done);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.rx = None,
            }
        }

        if !automatic || self.busy() {
            return;
        }
        // Nothing to look for while one is waiting to be installed.
        if matches!(
            self.status,
            Status::Available(_) | Status::Installing(_) | Status::Installed(_)
        ) {
            return;
        }
        match self.last_check.map(|t| t.elapsed()) {
            Some(age) if age < CHECK_EVERY => ctx.request_repaint_after(CHECK_EVERY - age),
            _ => self.check(ctx, false),
        }
    }

    fn finish(&mut self, done: Done) {
        self.status = match done {
            Done::Checked(Ok(Some(release))) => Status::Available(release),
            Done::Checked(Ok(None)) if self.manual => Status::UpToDate,
            Done::Checked(Err(e)) if self.manual => Status::Failed(e),
            // A quiet check that found nothing, or couldn't reach anything,
            // has nothing worth saying.
            Done::Checked(_) => Status::Idle,
            Done::Installed(Ok(target)) => Status::Installed(target),
            Done::Installed(Err(e)) => {
                if let Status::Installing(r) = &self.status {
                    self.failed_release = Some(r.clone());
                }
                Status::Failed(format!("Couldn't update: {e}"))
            }
        };
    }

    /// The release a banner should be offering, if any.
    pub fn banner(&self) -> Option<&Release> {
        match &self.status {
            Status::Available(r) if self.dismissed.as_deref() != Some(&r.version) => Some(r),
            Status::Installing(r) => Some(r),
            _ => None,
        }
    }
}

/// Automatic checks are for released builds. A debug build carries whatever
/// version `Cargo.toml` says, which is always behind the last release, and the
/// screenshot harness must never draw a banner.
pub fn automatic_allowed() -> bool {
    !cfg!(debug_assertions) && std::env::var_os("FLODO_DEMO").is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> Manifest {
        serde_json::from_str(&format!(
            r#"{{
                "version": "{version}",
                "url": "https://github.com/michellemayes/flodo/releases/tag/v{version}",
                "assets": {{
                    "macos-universal": {{"url": "https://example.test/m.zip", "sha256": "aa"}},
                    "linux-x86_64": {{"url": "https://example.test/l.tar.gz", "sha256": "bb"}},
                    "windows-x86_64": {{"url": "https://example.test/w.zip", "sha256": "cc"}}
                }}
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn versions_compare_as_numbers_not_strings() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("v0.2.0", "0.1.99"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.3", "0.1.3"));
        assert!(!is_newer("0.1.2", "0.1.3"));
    }

    #[test]
    fn a_version_that_isnt_three_numbers_is_never_newer() {
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("1.2", "0.1.0"));
        assert!(!is_newer("1.2.3.4", "0.1.0"));
        assert!(!is_newer("1.2.3-beta", "0.1.0"));
        assert!(!is_newer("9.9.9", "not-a-version"));
    }

    #[test]
    fn the_same_or_an_older_release_is_nothing_to_offer() {
        assert_eq!(release_from(manifest("0.1.3"), "0.1.3"), None);
        assert_eq!(release_from(manifest("0.1.2"), "0.1.3"), None);
    }

    #[test]
    fn a_newer_release_carries_this_platforms_archive() {
        let r = release_from(manifest("0.2.0"), "0.1.3").unwrap();
        assert_eq!(r.version, "0.2.0");
        assert!(r.page.ends_with("/v0.2.0"));
        let want = PLATFORM.map(|p| manifest("0.2.0").assets[p].clone());
        assert_eq!(r.asset, want);
    }

    /// The test binary lives in target/, not in Flodo.app, so on macOS it
    /// must refuse to try replacing itself and fall back to the page.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_loose_binary_on_macos_is_not_installable() {
        assert!(install_target().is_err());
        assert!(
            !release_from(manifest("9.0.0"), "0.1.0")
                .unwrap()
                .installable
        );
    }

    #[test]
    fn a_manifest_without_assets_still_parses() {
        let m: Manifest =
            serde_json::from_str(r#"{"version":"1.0.0","url":"https://x.test"}"#).unwrap();
        let r = release_from(m, "0.1.0").unwrap();
        assert!(r.asset.is_none());
        assert!(!r.installable);
    }

    #[test]
    fn sha256_matches_a_known_digest() {
        let dir = std::env::temp_dir().join(format!("flodo-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_hex(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_quiet_check_that_fails_says_nothing_but_a_manual_one_does() {
        let mut u = Updater::default();
        u.finish(Done::Checked(Err("offline".into())));
        assert_eq!(u.status, Status::Idle);

        u.manual = true;
        u.finish(Done::Checked(Err("offline".into())));
        assert_eq!(u.status, Status::Failed("offline".into()));

        u.finish(Done::Checked(Ok(None)));
        assert_eq!(u.status, Status::UpToDate);
    }

    #[test]
    fn a_dismissed_banner_stays_dismissed_for_that_version_only() {
        let mut u = Updater::default();
        let r = release_from(manifest("0.2.0"), "0.1.0").unwrap();
        u.finish(Done::Checked(Ok(Some(r.clone()))));
        assert_eq!(u.banner(), Some(&r));
        u.dismissed = Some("0.2.0".into());
        assert_eq!(u.banner(), None);

        let newer = release_from(manifest("0.3.0"), "0.1.0").unwrap();
        u.finish(Done::Checked(Ok(Some(newer.clone()))));
        assert_eq!(u.banner(), Some(&newer));
    }

    #[test]
    fn a_failed_install_remembers_what_it_was_installing() {
        let mut u = Updater::default();
        let r = release_from(manifest("0.2.0"), "0.1.0").unwrap();
        u.status = Status::Installing(r.clone());
        u.finish(Done::Installed(Err("disk full".into())));
        assert_eq!(
            u.status,
            Status::Failed("Couldn't update: disk full".into())
        );
        assert_eq!(u.failed_release, Some(r));
    }
}
