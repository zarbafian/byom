//! Per-surface Unix-domain sockets (§14.5 authority surfaces; kovee's
//! hardened control-socket discipline): one socket per surface —
//! `governance.sock`, `candidate.sock`, `participant.sock`,
//! `runtime.sock`, `projection.sock` — each `0600` inside a `0700`
//! per-user runtime directory. Peer authentication (`SO_PEERCRED`
//! same-UID) happens per connection in the serve loop; the candidate and
//! RUNTIME sockets additionally take a mandatory channel-token preamble
//! line (the offer-scoped candidate credential; the episode-scoped or
//! allocation-scoped workload token of R30/R33/R35).

use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use crate::peercred::current_uid;

/// Which socket a request arrived on. The registry's (operation,surface)
/// rows against THIS value are the dispatch truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketSurface {
    Governance,
    Candidate,
    Participant,
    Runtime,
    Projection,
}

impl SocketSurface {
    pub const ALL: [SocketSurface; 5] = [
        SocketSurface::Governance,
        SocketSurface::Candidate,
        SocketSurface::Participant,
        SocketSurface::Runtime,
        SocketSurface::Projection,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SocketSurface::Governance => "governance",
            SocketSurface::Candidate => "candidate",
            SocketSurface::Participant => "participant",
            SocketSurface::Runtime => "runtime",
            SocketSurface::Projection => "projection",
        }
    }

    pub fn registry_surface(self) -> bpp_core::registry::Surface {
        match self {
            SocketSurface::Governance => bpp_core::registry::Surface::Governance,
            SocketSurface::Candidate => bpp_core::registry::Surface::Candidate,
            SocketSurface::Participant => bpp_core::registry::Surface::Participant,
            SocketSurface::Runtime => bpp_core::registry::Surface::Runtime,
            SocketSurface::Projection => bpp_core::registry::Surface::Projection,
        }
    }

    pub fn socket_file(self) -> String {
        format!("{}.sock", self.name())
    }
}

/// The runtime directory holding the sockets. In priority:
/// `$BYOM_RUNTIME_DIR` (exact directory — tests and service units), else
/// `$XDG_RUNTIME_DIR/byom` (private `0700` per-user tmpfs), else a
/// UID-scoped temp directory. Daemon and CLI resolve through this one
/// function so they always agree.
pub fn socket_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BYOM_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(rt) if !rt.is_empty() => PathBuf::from(rt).join("byom"),
        _ => std::env::temp_dir().join(format!("byom-{}", current_uid())),
    }
}

pub fn socket_path(surface: SocketSurface) -> PathBuf {
    socket_dir().join(surface.socket_file())
}

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("socket io: {0}")]
    Io(#[from] std::io::Error),
}

/// Creates the `0700` runtime directory, removes a stale socket file,
/// and binds the listener with the socket file at `0600`.
pub fn bind(surface: SocketSurface) -> Result<(UnixListener, PathBuf), BindError> {
    let dir = socket_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let path = socket_path(surface);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, path))
}
