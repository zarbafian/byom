//! The embedded C3a tools document — the contract. The frozen bundle
//! (`mcp/byom-mcp.tools.json` v0.1.1, D-RT-2/RT-14) is embedded at
//! build time and parsed once at startup: tool names, ops, gating
//! flags, and input schemas all come from it, nothing is hand-copied.
//! Each tool's op is cross-checked against bpp-core's frozen
//! (operation,surface) registry — the same rows byomd dispatches with —
//! so the served surface and the RT-01 meta class are registry truth,
//! never assumptions in this crate. A document this loader (or the
//! schema interpreter) cannot fully enforce makes the server refuse to
//! start rather than serve a weaker contract.

use bpp_core::registry::{self, OpClass, Surface};
use serde_json::Value;

use crate::validate;

/// The tools document, embedded verbatim at build time.
pub const DOCUMENT_JSON: &str = include_str!("../../../mcp/byom-mcp.tools.json");

/// The meta-schema's per-profile tool-count pins
/// (spec/schemas/mcp-tools.schema.json: minItems == maxItems).
pub const CANDIDATE_TOOL_COUNT: usize = 3;
pub const PARTICIPANT_TOOL_COUNT: usize = 34;

/// Which C3a profile this server instance serves. Exactly one — the two
/// bindings are different credentials with different scopes and never
/// mix (amendment A4: the candidate channel closes at admission and
/// never converts in place).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Candidate,
    Participant,
}

impl Profile {
    pub fn parse(s: &str) -> Option<Profile> {
        match s {
            "candidate" => Some(Profile::Candidate),
            "participant" => Some(Profile::Participant),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Candidate => "candidate",
            Profile::Participant => "participant",
        }
    }

    /// The registry surfaces a tool op of this profile may bind, in
    /// resolution order. Governance, runtime, and admin surfaces are
    /// NEVER here — an op resolvable only there refuses to load
    /// (the document's excluded_surfaces assertion, made structural).
    fn allowed_surfaces(self) -> &'static [Surface] {
        match self {
            Profile::Candidate => &[Surface::Candidate],
            Profile::Participant => &[
                Surface::Participant,
                Surface::Projection,
                Surface::Originating,
            ],
        }
    }

    /// The binding's sender-constraint label in the document, checked at
    /// load so the credential discipline the server enforces is the one
    /// the document declares.
    fn sender_constraint(self) -> &'static str {
        match self {
            Profile::Candidate => "one_membership_offer",
            Profile::Participant => "participant_channel_credential",
        }
    }

    fn pinned_count(self) -> usize {
        match self {
            Profile::Candidate => CANDIDATE_TOOL_COUNT,
            Profile::Participant => PARTICIPANT_TOOL_COUNT,
        }
    }
}

/// One tool row of the served profile.
pub struct Tool {
    /// The MCP tool name (`byom_<registry-op>`).
    pub name: String,
    /// The frozen registry operation the tool dispatches to.
    pub op: String,
    /// `access: "gated"` (every mutation) vs `access: "safe_to_allow"`
    /// (the projection/originating reads).
    pub gated: bool,
    /// The document description, verbatim — it carries the read-only vs
    /// gated marking the harness shows the operator.
    pub description: String,
    /// The closed input schema (op request args minus the bridge-derived
    /// envelope {version, op, meta}), verbatim.
    pub input_schema: Value,
    /// The registry surface this profile dispatches the op on — decides
    /// which byomd socket the bridge dials.
    pub surface: Surface,
    /// The registry meta class (RT-01): read (no meta), create (meta
    /// without expected_revision), update (meta with it).
    pub class: OpClass,
}

/// The parsed profile: the exact tool list, in document order, plus the
/// pinned BPP protocol version.
pub struct Document {
    pub protocol_version: String,
    pub profile: Profile,
    pub tools: Vec<Tool>,
}

impl Document {
    /// Looks a tool up by name; absence means the tool does not exist
    /// (deny-by-absence).
    pub fn tool(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.name == name)
    }
}

/// Parses and cross-checks the embedded document for one profile.
pub fn load(profile: Profile) -> Result<Document, String> {
    let root: Value =
        serde_json::from_str(DOCUMENT_JSON).map_err(|e| format!("tools document: {e}"))?;
    if root.get("document").and_then(Value::as_str) != Some("byom-mcp.tools") {
        return Err("embedded file is not the byom-mcp.tools document".to_owned());
    }
    let protocol_version = root
        .get("bpp_protocol_version")
        .and_then(Value::as_str)
        .ok_or("bpp_protocol_version missing")?
        .to_owned();
    if protocol_version != bpp_core::PROTOCOL_VERSION {
        return Err(format!(
            "document pins BPP {protocol_version} but this build speaks {}",
            bpp_core::PROTOCOL_VERSION
        ));
    }
    let profiles = root
        .get("profiles")
        .and_then(Value::as_object)
        .ok_or("profiles missing")?;
    if profiles.len() != 2
        || !profiles.contains_key("candidate")
        || !profiles.contains_key("participant")
    {
        return Err("expected exactly the candidate and participant profiles".to_owned());
    }
    let section = profiles
        .get(profile.as_str())
        .ok_or("selected profile missing")?;
    let constraint = section
        .get("binding")
        .and_then(|b| b.get("sender_constraint"))
        .and_then(Value::as_str);
    if constraint != Some(profile.sender_constraint()) {
        return Err(format!(
            "{} binding is not sender-constrained to {:?}",
            profile.as_str(),
            profile.sender_constraint()
        ));
    }
    let rows = section
        .get("tools")
        .and_then(Value::as_array)
        .ok_or("profile tools missing")?;
    let mut tools: Vec<Tool> = Vec::with_capacity(rows.len());
    for row in rows {
        let tool = parse_tool(row, profile)?;
        if tools.iter().any(|t| t.name == tool.name) {
            return Err(format!("duplicate tool {}", tool.name));
        }
        tools.push(tool);
    }
    // The meta-schema's exact-count pin, re-checked here so a widened or
    // truncated embedded document cannot serve.
    if tools.len() != profile.pinned_count() {
        return Err(format!(
            "the {} profile lists {} tools; the meta-schema pins {}",
            profile.as_str(),
            tools.len(),
            profile.pinned_count()
        ));
    }
    Ok(Document {
        protocol_version,
        profile,
        tools,
    })
}

fn parse_tool(row: &Value, profile: Profile) -> Result<Tool, String> {
    let name = member_str(row, "name")?;
    let op = member_str(row, "op")?;
    let description = member_str(row, "description")?;
    let gated = match member_str(row, "access")?.as_str() {
        "gated" => true,
        "safe_to_allow" => false,
        other => return Err(format!("tool {name}: unknown access {other:?}")),
    };
    if name != format!("byom_{op}") {
        return Err(format!("tool {name}: name does not derive from op {op:?}"));
    }
    // The op must sit on a registry surface this profile may dispatch;
    // the row's surface and class are the dispatch truth (§14.6/§14.7,
    // RT-01). An op resolvable only on governance/runtime refuses here.
    let row_spec = profile
        .allowed_surfaces()
        .iter()
        .find_map(|surface| registry::lookup(&op, *surface));
    let Some(spec) = row_spec else {
        return Err(format!(
            "tool {name}: op {op:?} has no registry row on a {} surface",
            profile.as_str()
        ));
    };
    // Gating agreement: registry reads are safe_to_allow, mutations
    // gated (the akson-mcp marking, conformance-checked on the document
    // and re-checked here against the registry).
    if gated == (spec.class == OpClass::Read) {
        return Err(format!(
            "tool {name}: access marking disagrees with the registry class"
        ));
    }
    let input_schema = row
        .get("input_schema")
        .cloned()
        .ok_or_else(|| format!("tool {name}: input_schema missing"))?;
    validate::check_supported(&input_schema).map_err(|e| format!("tool {name}: {e}"))?;
    // The bridge-derived envelope is never tool input (RT-16): a schema
    // that declares an envelope member would let the channel discipline
    // be argued with — refuse to serve it.
    if let Some(properties) = input_schema.get("properties").and_then(Value::as_object) {
        for member in ["version", "op", "meta"] {
            if properties.contains_key(member) {
                return Err(format!(
                    "tool {name}: input schema declares envelope member {member:?}"
                ));
            }
        }
    }
    Ok(Tool {
        name,
        op,
        gated,
        description,
        input_schema,
        surface: spec.surface,
        class: spec.class,
    })
}

fn member_str(row: &Value, key: &str) -> Result<String, String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("tool row: {key} missing or not a string"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_candidate_profile_loads_exactly_three_tools() {
        let doc = load(Profile::Candidate).unwrap();
        assert_eq!(doc.protocol_version, "0.2");
        assert_eq!(doc.tools.len(), 3);
        let names: Vec<&str> = doc.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "byom_membership_refuse",
                "byom_membership_accept",
                "byom_candidate_self_policy_propose",
            ]
        );
        for tool in &doc.tools {
            assert_eq!(tool.surface, Surface::Candidate, "{}", tool.name);
            assert!(
                tool.gated,
                "{}: every candidate tool is a mutation",
                tool.name
            );
        }
        let accept = doc.tool("byom_membership_accept").unwrap();
        assert_eq!(accept.class, OpClass::Update);
        let propose = doc.tool("byom_candidate_self_policy_propose").unwrap();
        assert_eq!(propose.class, OpClass::Create);
    }

    #[test]
    fn the_participant_profile_loads_exactly_thirty_four_tools() {
        let doc = load(Profile::Participant).unwrap();
        assert_eq!(doc.tools.len(), 34);
        // Gating marking lives in the document descriptions; the parsed
        // flag must agree with the text the harness will show.
        for tool in &doc.tools {
            assert_eq!(
                tool.gated,
                tool.description.contains("gated"),
                "{} marking drifted from its description",
                tool.name
            );
            assert_eq!(
                !tool.gated,
                tool.description.contains("safe to allow"),
                "{} read marking drifted",
                tool.name
            );
        }
        // Surface routing comes from the registry rows.
        assert_eq!(
            doc.tool("byom_activity_open").unwrap().surface,
            Surface::Participant
        );
        assert_eq!(
            doc.tool("byom_society_show").unwrap().surface,
            Surface::Projection
        );
        assert_eq!(
            doc.tool("byom_idempotency_result").unwrap().surface,
            Surface::Originating
        );
        // RT-01 classes straight from the registry.
        assert_eq!(
            doc.tool("byom_wake_intent_submit").unwrap().class,
            OpClass::Create
        );
        assert_eq!(
            doc.tool("byom_activity_close").unwrap().class,
            OpClass::Update
        );
        assert_eq!(doc.tool("byom_events_read").unwrap().class, OpClass::Read);
    }

    #[test]
    fn absent_tools_stay_absent_in_both_profiles() {
        let candidate = load(Profile::Candidate).unwrap();
        let participant = load(Profile::Participant).unwrap();
        // Real registry ops outside each profile must not appear —
        // governance/runtime ops in neither, and never across profiles.
        for op in [
            "participant_admit",
            "membership_offer",
            "mandate_issue",
            "society_bootstrap",
            "charter_finalize",
        ] {
            assert!(candidate.tool(&format!("byom_{op}")).is_none(), "{op}");
            assert!(participant.tool(&format!("byom_{op}")).is_none(), "{op}");
        }
        // The D-RT-2 null-bound removals stay removed.
        for op in [
            "engram_propose",
            "engram_read",
            "engram_search",
            "budget_show",
        ] {
            assert!(participant.tool(&format!("byom_{op}")).is_none(), "{op}");
        }
        // Profiles never bleed into each other (A4).
        assert!(candidate.tool("byom_activity_open").is_none());
        assert!(participant.tool("byom_membership_accept").is_none());
    }
}
