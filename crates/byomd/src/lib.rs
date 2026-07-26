//! `byomd` — the byom reference daemon (personal profile): one
//! plain-SQLite WAL store driven through the §15.3 authority mutation
//! journal (developer-recovery witness, honestly labeled), five
//! per-surface Unix sockets (`governance.sock`, `candidate.sock`,
//! `participant.sock`, `runtime.sock`, `projection.sock` — `0600` in a
//! `0700` dir, `SO_PEERCRED` same-UID), and the frozen
//! (operation,surface) registry as the dispatch truth.
//!
//! What a client writes (one line in, one line out):
//! ```text
//! {"version":"0.2","op":"hello"}
//! → {"outcome":"ok","result":{"versions":["0.2"],"surface":"governance", ...}}
//! ```
//!
//! On the candidate socket a channel-token preamble line precedes the
//! request line (the offer-scoped credential of §7.4).

pub mod cand_ops;
pub mod channel;
pub mod dispatch;
pub mod effect_ops;
pub mod episode_ops;
pub mod gov_authority;
pub mod gov_decision;
pub mod gov_ops;
pub mod host_config;
pub mod host_ops;
pub mod host_recovery;
pub mod part_common;
pub mod part_ops;
pub mod peercred;
pub mod reads;
pub mod socket;
pub mod state;
pub mod work_ops;

pub use dispatch::{AbortSpec, Daemon};
pub use socket::SocketSurface;
