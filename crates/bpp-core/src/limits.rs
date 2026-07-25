//! The §14.9 conformance-tested initial limits, plus the family-profile
//! caps (family-vectors/PROFILE.md §1, profile-pinned decision 1).

/// Request envelope cap: 256 KiB, inclusive (§14.9).
pub const REQUEST_MAX_BYTES: usize = 262_144;
/// Response cap: 1 MiB, inclusive (§14.9).
pub const RESPONSE_MAX_BYTES: usize = 1_048_576;
/// Identifier byte cap (§14.9).
pub const IDENTIFIER_MAX_BYTES: usize = 128;
/// At most 256 list items per mutation (§14.9).
pub const MUTATION_LIST_ITEMS_MAX: usize = 256;
/// At most 512 events per page (§14.9).
pub const EVENTS_PAGE_ITEMS_MAX: u64 = 512;
/// JSON container nesting depth cap (PROFILE §1, pinned decision 1).
pub const JSON_DEPTH_MAX: usize = 64;
/// JSON value (node) cap per document (PROFILE §1, pinned decision 1).
pub const JSON_NODES_MAX: usize = 65_536;
/// The revision of the advertised limit set (`protocol_info`).
pub const LIMITS_REVISION: u64 = 0;
