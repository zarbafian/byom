//! byomd entry point.
//!
//! ```text
//! byomd [--data-dir <dir>]
//! ```
//!
//! Sockets live under `$XDG_RUNTIME_DIR/byom/` (`$BYOM_RUNTIME_DIR`
//! overrides); state lives at `<data-dir>/byom.db` with the
//! developer-recovery witness beside it (default `$XDG_DATA_HOME/byom`,
//! else `~/.local/share/byom`; `$BYOM_DATA_DIR` overrides).

use std::path::PathBuf;
use std::sync::Arc;

use byomd::{AbortSpec, Daemon, SocketSurface};

fn data_dir(cli_override: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = cli_override {
        return dir;
    }
    if let Some(dir) = std::env::var_os("BYOM_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("byom");
        }
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local/share/byom"),
        None => PathBuf::from(".byom"),
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut dir_flag = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                dir_flag = Some(PathBuf::from(args.next().ok_or("--data-dir needs a path")?));
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let dir = data_dir(dir_flag);
    let store = byom_store::Store::open(&dir).map_err(|e| format!("open store: {e}"))?;
    if store.sealed() {
        eprintln!(
            "byomd: endpoint is sealed_diagnostic ({}); non-diagnostic surfaces are closed",
            store.seal_reason().unwrap_or_default()
        );
    }
    byomd::gov_ops::ensure_channel_files(&store);
    let daemon = Arc::new(Daemon::new(store, AbortSpec::from_env()));
    let mut bound = Vec::new();
    let mut listeners = Vec::new();
    for surface in SocketSurface::ALL {
        let (listener, path) =
            byomd::socket::bind(surface).map_err(|e| format!("bind {}: {e}", surface.name()))?;
        bound.push(path.display().to_string());
        listeners.push((listener, surface));
    }
    println!(
        "byomd: personal profile; store {}; witness developer-recovery; listening on {}",
        dir.join("byom.db").display(),
        bound.join(", ")
    );
    let mut handles = Vec::new();
    for (listener, surface) in listeners {
        let daemon = Arc::clone(&daemon);
        handles.push(std::thread::spawn(move || {
            daemon.serve(listener, surface);
        }));
    }
    for handle in handles {
        let _ = handle.join();
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("byomd: {e}");
        std::process::exit(1);
    }
}
