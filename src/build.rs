//! `pleamar-wm build` (Super+B): build mode, to build your desktop by asking.
//!
//! The first time it starts the build scene (build/: a pleamar scene, its
//! logic and its AI engine), with the mode on; while it runs, it tells it to
//! turn the mode off or on again. Its files go inside this program and are
//! written where they run (~/.local/share/pleamar-wm/build), with the
//! engine's packages, which its logic fetches there the first time.

const FILES: &[(&str, &str)] = &[
    ("build.plm", include_str!("../build/build.plm")),
    ("panel.plm", include_str!("../build/panel.plm")),
    ("build.luau", include_str!("../build/build.luau")),
    ("logic/engine.luau", include_str!("../build/logic/engine.luau")),
    ("logic/creations.luau", include_str!("../build/logic/creations.luau")),
    ("logic/draw.luau", include_str!("../build/logic/draw.luau")),
    ("lang/es.plm", include_str!("../build/lang/es.plm")),
    ("agent/worker.mjs", include_str!("../build/agent/worker.mjs")),
    ("agent/tools.mjs", include_str!("../build/agent/tools.mjs")),
    ("agent/package.json", include_str!("../build/agent/package.json")),
    ("agent/package-lock.json", include_str!("../build/agent/package-lock.json")),
    ("agent/sandbox", include_str!("../build/agent/sandbox")),
];

/// Where the build scene listens, as pleamar names it: `PLEAMAR_SOCKETS`
/// (what pleamar-wm gives the programs it starts) or `$XDG_RUNTIME_DIR/pleamar`.
fn socket() -> std::path::PathBuf {
    let dir = match std::env::var("PLEAMAR_SOCKETS").ok().filter(|d| !d.is_empty()) {
        Some(d) => std::path::PathBuf::from(d),
        None => std::path::Path::new(&std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into())).join("pleamar"),
    };
    dir.join("build.sock")
}

/// Where its files are written: next to the engine's packages, which stay.
pub fn dir() -> String {
    let data = std::env::var("XDG_DATA_HOME").ok().filter(|v| !v.is_empty()).or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.local/share"))).unwrap_or_else(|| "/tmp".into());
    format!("{data}/pleamar-wm/build")
}

/// Written only where they changed: the engine's lock, rewritten, would look
/// newer than its packages and fetch them again.
fn materialize(dir: &str) -> Result<(), String> {
    for (name, text) in FILES {
        let path = std::path::Path::new(dir).join(name);
        if std::fs::read_to_string(&path).is_ok_and(|t| t == *text) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        if name.ends_with("sandbox") {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
    }
    Ok(())
}

/// Its files where they run, for the control centre too (it signs in with
/// the same engine).
pub fn prepare() {
    if let Err(e) = materialize(&dir()) {
        eprintln!("build · {e}");
    }
}

pub fn run(args: &[String]) {
    // Running: told. `pleamar-wm build` toggles; `on` and `off` say which.
    let order = match args.first().map(String::as_str) {
        Some("off") => "emit leave",
        Some("on") => "fact on true",
        _ => "emit toggle",
    };
    if let Ok(mut s) = std::os::unix::net::UnixStream::connect(socket()) {
        use std::io::Write;
        let _ = writeln!(s, "{order}");
        return;
    }
    if order == "emit leave" {
        return;
    }
    let dir = dir();
    if let Err(e) = materialize(&dir) {
        eprintln!("build · {e}");
        std::process::exit(1);
    }
    if let Err(e) = std::env::set_current_dir(&dir) {
        eprintln!("build · {dir}: {e}");
        std::process::exit(1);
    }
    // SAFETY: one thread still: nothing else reads the environment yet.
    unsafe { std::env::set_var("BUILD_ON", "1") };
    pleamar::run_with(vec!["--scene".to_owned(), format!("{dir}/build.plm"), "--no-hud".to_owned()]);
}
