//! Advisory whole-file locks (`flock(2)`) — the inter-process
//! serialization the §15.3 witness compare-and-append and exclusive
//! data-directory ownership need. A process-local `Mutex` orders threads
//! inside one daemon; it says nothing about a second daemon, so every
//! cross-process critical section takes one of these.
//!
//! The lock lives on the open file description, so it is released by the
//! kernel when the process dies — a crashed daemon never wedges the data
//! directory, and a `fork`ed child would SHARE it (which is why the
//! competing-CAS tests spawn real processes, never forks).
//!
//! What you write:
//! ```
//! use byom_store::flock::FileLock;
//! let path = std::env::temp_dir().join(format!("bl-{}", std::process::id()));
//! let file = std::fs::OpenOptions::new()
//!     .create(true).read(true).write(true).truncate(false).open(&path).unwrap();
//! let guard = FileLock::exclusive(file).unwrap();   // held until dropped
//! drop(guard);
//! std::fs::remove_file(&path).unwrap();
//! ```

use std::fs::File;
use std::os::unix::io::AsRawFd as _;

/// An exclusive advisory lock held for the guard's lifetime.
#[derive(Debug)]
pub struct FileLock {
    file: File,
}

fn flock(file: &File, op: libc::c_int) -> std::io::Result<()> {
    // SAFETY: `flock` takes a valid file descriptor and an operation
    // constant; the fd is owned by `file` and outlives the call. The
    // return code is checked.
    #[allow(unsafe_code)]
    let rc = unsafe { libc::flock(file.as_raw_fd(), op) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

impl FileLock {
    /// Blocks until this process owns the file exclusively.
    pub fn exclusive(file: File) -> std::io::Result<FileLock> {
        flock(&file, libc::LOCK_EX)?;
        Ok(FileLock { file })
    }

    /// Takes the lock or fails immediately (`WouldBlock`) when another
    /// process holds it — the exclusive-ownership test.
    pub fn try_exclusive(file: File) -> std::io::Result<FileLock> {
        flock(&file, libc::LOCK_EX | libc::LOCK_NB)?;
        Ok(FileLock { file })
    }

    pub fn file(&self) -> &File {
        &self.file
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = flock(&self.file, libc::LOCK_UN);
    }
}

/// Opens (creating if absent) a lock file for `path`.
pub fn open_lock_file(path: &std::path::Path) -> std::io::Result<File> {
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_second_lock_on_the_same_description_family_is_refused_across_files() {
        let path = std::env::temp_dir().join(format!("byom-flock-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let held = FileLock::exclusive(open_lock_file(&path).unwrap()).unwrap();
        // A DIFFERENT open file description in the same process still
        // contends (flock is per-description, not per-process).
        let second = FileLock::try_exclusive(open_lock_file(&path).unwrap());
        assert!(second.is_err(), "a second description must not take it");
        drop(held);
        FileLock::try_exclusive(open_lock_file(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
