//! Unix peer-credential authentication (the family control-socket
//! discipline): the daemon reads the connecting process's credentials
//! with `SO_PEERCRED` and refuses any peer whose UID is not its own.
//! This is the B1 developer-profile channel authentication — honestly
//! labeled: same-UID possession stands in for the hosted profile's
//! per-principal credentials.

use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;

/// The credentials of the process on the other end of a Unix socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCredentials {
    pub pid: i32,
    pub uid: u32,
    pub gid: u32,
}

/// Why a local peer failed authentication.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("could not read peer credentials: {0}")]
    Credentials(String),
    /// Generic so it leaks nothing about who connected.
    #[error("local peer is not authorized")]
    Unauthorized,
}

/// The effective UID of this process.
pub fn current_uid() -> u32 {
    // SAFETY: geteuid is always safe and cannot fail.
    #[allow(unsafe_code)]
    unsafe {
        libc::geteuid()
    }
}

/// Reads the connected peer's credentials via `SO_PEERCRED`.
pub fn peer_credentials(stream: &UnixStream) -> Result<PeerCredentials, AuthError> {
    // SAFETY: getsockopt writes a `ucred` of `len` bytes into `cred` for
    // a valid socket fd; on success `cred` is fully initialized. The
    // return code is checked.
    #[allow(unsafe_code)]
    unsafe {
        let mut cred: libc::ucred = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc = libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast::<libc::c_void>(),
            &mut len,
        );
        if rc != 0 {
            return Err(AuthError::Credentials(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        Ok(PeerCredentials {
            pid: cred.pid,
            uid: cred.uid,
            gid: cred.gid,
        })
    }
}

/// Authenticates a local peer: its UID must equal `expected_uid`.
pub fn authenticate_same_uid(
    stream: &UnixStream,
    expected_uid: u32,
) -> Result<PeerCredentials, AuthError> {
    let cred = peer_credentials(stream)?;
    if cred.uid == expected_uid {
        Ok(cred)
    } else {
        Err(AuthError::Unauthorized)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn same_uid_authenticates_and_a_foreign_uid_is_refused() {
        let (a, _b) = UnixStream::pair().unwrap();
        authenticate_same_uid(&a, current_uid()).unwrap();
        let other = current_uid().wrapping_add(1);
        assert_eq!(
            authenticate_same_uid(&a, other),
            Err(AuthError::Unauthorized)
        );
    }
}
