//! BPP wire core for byom (DESIGN.md §14, the frozen B0.1 bundle): strict
//! I-JSON acceptance, the request/success/failure envelopes, the closed
//! 29-kind problem enum, typed `DigestRef`s, the canonical
//! `IdempotencyDomain` digest per the ratified family profile
//! (`family-vectors/PROFILE.md` §5, D-R0-1), and the embedded
//! (operation,surface) registry that is the dispatch truth.
//!
//! What you write (one request in, one typed op out):
//! ```
//! use bpp_core::envelope::RawRequest;
//! let value = bpp_core::ijson::parse_request(
//!     r#"{"version":"0.2","op":"hello"}"#.as_bytes()).unwrap();
//! let req = RawRequest::from_value(&value).unwrap();
//! assert_eq!(req.op, "hello");
//! assert!(req.meta.is_none()); // reads never carry meta
//! ```

pub mod bpa1;
pub mod canonical;
pub mod digest;
pub mod envelope;
pub mod hostint;
pub mod idempotency;
pub mod ijson;
pub mod limits;
pub mod ops;
pub mod problem;
pub mod registry;
pub mod time;

/// The one protocol minor version this bundle speaks (§14.1).
pub const PROTOCOL_VERSION: &str = "0.2";
