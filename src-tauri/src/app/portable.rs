// ============================================
// Portable mode
//
// Keeps every writable artifact inside `<exe dir>/data` instead of the
// per-user locations (AppData on Windows, XDG dirs on Linux/macOS):
//
//   <exe dir>/data/config/         window-state.json, wsl-servers.json, ...
//   <exe dir>/data/logs/           tauri-plugin-log output
//   <exe dir>/data/webview/        WebView2 / WebKit profile (localStorage,
//                                  IndexedDB — i.e. all UI settings)
//   <exe dir>/data/opencode/       XDG roots handed to the backend:
//       data/  config/  state/  cache/
//
// Enabled when any of the following is true (checked once, cached):
//   * the crate is built with `--features portable`
//   * `OPENCODEUI_PORTABLE=1` is set (`0` force-disables)
//   * a `portable.txt` marker sits next to the executable
//
// If the directory cannot be created/written (e.g. read-only medium,
// Program Files without elevation) portable mode disables itself and the
// app falls back to the regular per-user locations.
// ============================================

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Marker file next to the executable that turns portable mode on.
pub const MARKER_FILE: &str = "portable.txt";

/// Folder created next to the executable that holds all writable state.
pub const DATA_DIR_NAME: &str = "data";

fn parse_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_flag() -> Option<bool> {
    let value = std::env::var("OPENCODEUI_PORTABLE").ok()?;
    Some(parse_flag(&value))
}

/// Directory containing the running executable.
///
/// Linux AppImages are mounted under `/tmp/.mount_*`, so the real file
/// location is taken from `$APPIMAGE` when present.
pub fn exe_dir() -> Option<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(appimage);
        if let Some(parent) = path.parent() {
            return Some(parent.to_path_buf());
        }
    }
    let exe = std::env::current_exe().ok()?;
    exe.parent().map(Path::to_path_buf)
}

/// Portable data root: `<exe dir>/data`.
pub fn root() -> Option<PathBuf> {
    exe_dir().map(|dir| dir.join(DATA_DIR_NAME))
}

/// Subdirectory of the portable root, or `None` when portable mode is off.
fn sub_dir(kind: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    root().map(|dir| dir.join(kind))
}

pub fn config_dir() -> Option<PathBuf> {
    sub_dir("config")
}

pub fn log_dir() -> Option<PathBuf> {
    sub_dir("logs")
}

pub fn webview_dir() -> Option<PathBuf> {
    sub_dir("webview")
}

/// Backend XDG roots, laid out as `<root>/opencode/<kind>`.
pub fn opencode_dir(kind: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    root().map(|dir| dir.join("opencode").join(kind))
}

/// Bundled `opencode` binary shipped next to the executable, if present.
pub fn backend_binary() -> Option<PathBuf> {
    let dir = exe_dir()?;
    let names: &[&str] = if cfg!(windows) {
        &["opencode.exe", "opencode.cmd", "opencode.bat", "opencode"]
    } else {
        &["opencode"]
    };
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
}

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether portable mode is active for this process.
pub fn enabled() -> bool {
    *ENABLED.get_or_init(compute_enabled)
}

fn compute_enabled() -> bool {
    let build_flag = cfg!(feature = "portable");
    let marker = exe_dir()
        .map(|dir| dir.join(MARKER_FILE).is_file())
        .unwrap_or(false);

    let requested = match env_flag() {
        Some(value) => value,
        None => build_flag || marker,
    };
    if !requested {
        return false;
    }

    if prepare() {
        true
    } else {
        eprintln!(
            "[portable] cannot write to {:?}, falling back to per-user directories",
            root()
        );
        false
    }
}

fn writable_probe(dir: &Path) -> bool {
    let probe = dir.join(".write-test");
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

/// Create the portable layout. Returns false when the location is unusable.
fn prepare() -> bool {
    let Some(root) = root() else {
        return false;
    };

    let dirs = [
        root.clone(),
        root.join("config"),
        root.join("logs"),
        root.join("webview"),
        root.join("opencode").join("data"),
        root.join("opencode").join("config"),
        root.join("opencode").join("state"),
        root.join("opencode").join("cache"),
    ];

    for dir in dirs {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            eprintln!("[portable] create {} failed: {err}", dir.display());
            return false;
        }
    }

    writable_probe(&root)
}

fn set_if_unset(key: &str, value: PathBuf) {
    if std::env::var_os(key).is_some() {
        return;
    }
    std::env::set_var(key, value);
}

/// Prepend `<exe dir>` to `PATH` so `opencode` is found by user shells too.
fn prepend_exe_dir_to_path() {
    let Some(dir) = exe_dir() else { return };
    let mut paths = vec![dir];
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var("PATH", joined);
    }
}

/// Export the portable locations to this process (and thus to every child
/// process spawned later). Call once, before the Tauri app is built and
/// before `opencode serve` is spawned.
pub fn apply_process_env() {
    if !enabled() {
        return;
    }

    // opencode reads its global roots from XDG on every platform, falling
    // back to `$HOME/.local/{share,state}` / `$HOME/.config`. Setting the
    // XDG variables keeps auth, sessions, config and service registry local.
    set_if_unset(
        "XDG_DATA_HOME",
        opencode_dir("data").expect("portable enabled"),
    );
    set_if_unset(
        "XDG_CONFIG_HOME",
        opencode_dir("config").expect("portable enabled"),
    );
    set_if_unset(
        "XDG_STATE_HOME",
        opencode_dir("state").expect("portable enabled"),
    );
    set_if_unset(
        "XDG_CACHE_HOME",
        opencode_dir("cache").expect("portable enabled"),
    );

    if let Some(bin) = backend_binary() {
        set_if_unset("OPENCODE_BIN", bin);
    }
    prepend_exe_dir_to_path();

    // WebView2 honors this variable when no explicit user data folder is
    // passed to the environment; it must be set before the webview starts.
    #[cfg(target_os = "windows")]
    if let Some(dir) = webview_dir() {
        set_if_unset("WEBVIEW2_USER_DATA_FOLDER", dir);
    }

    eprintln!(
        "[portable] enabled, data directory: {}",
        root().map(|p| p.display().to_string()).unwrap_or_default()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_dir_is_absolute() {
        let dir = exe_dir().expect("test binary has a parent directory");
        assert!(dir.is_absolute());
    }

    #[test]
    fn root_ends_with_data() {
        let root = root().expect("test binary has a parent directory");
        assert_eq!(
            root.file_name().and_then(|n| n.to_str()),
            Some(DATA_DIR_NAME)
        );
    }

    #[test]
    fn flag_parsing_is_lenient() {
        for truthy in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(parse_flag(truthy), "{truthy:?} should enable portable mode");
        }
        for falsy in ["0", "false", "no", "", "off"] {
            assert!(
                !parse_flag(falsy),
                "{falsy:?} should not enable portable mode"
            );
        }
    }
}
