# Amendment A9: the governance owner enum is `byom | none`

Status: proposed (becomes normative on acceptance)

Date: 2026-07-27

Amends: `byom/DESIGN.md` §16.6 and §25 at sha256
`ccea384ff931bcf45d30df680b86835ac682006072a07ef2f34f565eba5fa501`, and
supersedes the enum clause of amendment A2
(`design/2026-07-25-amendment-family-contract.md`). The design text is
byte-frozen and pinned by the family lock; until it is re-cut, **this record
overrides it wherever they conflict** — the same mechanism A1–A8 use.

## A9.1 — The third owner arm is withdrawn

§16.6's `KoveeGovernanceOwnerBinding.governance_owner` reads
`sage | byom | none`. The first arm named a discarded predecessor design.
Amendment A2 kept it "for spec fidelity" and recorded that no milestone would
exercise it. That was the wrong trade, and the reason is not tidiness:

- **No writer.** No operation, saga, or kernel transition in either repo ever
  produced the value.
- **No reader.** Every consumer branched on `byom` or `none`; the third arm
  fell through to a `_ => false` or was excluded by an invariant.
- **A vacuous proof obligation.** `GreenfieldEnablement.tla` carried an
  invariant asserting the arm was never taken. An invariant over a value no
  implementation can produce checks nothing; it only makes the model look
  more thorough than it is.
- **A live widening surface.** A closed enum is a security boundary. Carrying
  an owner arm that nothing authorizes means every reviewer of every future
  change has to re-derive that it stays unreachable.

Byom is this family's governance layer, reached over the Byom Participation
Protocol. A governed scope is therefore in exactly one of two states: **byom
owns it, or nothing does.** The enum says exactly that and nothing else.

## A9.2 — How the narrowing lands (schema version, not an edit)

Narrowing an enum is a breaking change, and `spec/README.md`'s bundle-freeze
rule is explicit: *published schemas are immutable; any change is a new schema
version file*. So this follows the RT-06 successor precedent exactly:

- **New:** `spec/governed-work/kovee-governance-owner-binding-v2.schema.json`
  — enum `byom | none`, the owning `oneOf` arm pinned to `const: "byom"`,
  `$id` version path unchanged at `/bpp/v0/`, the version in the filename.
- **Unchanged:** `spec/governed-work/kovee-governance-owner-binding.schema.json`
  stays published byte-for-byte, and so does its
  `…-sage-valid` vector, which still validates against it. v1 is the
  historical publication; it is not a lie, and rewriting it would make it one.
- **New vectors:** `…-v2-byom-valid`, `…-v2-none-valid`, and
  `…-v2-withdrawn-owner-arm-invalid`. The last feeds the exact value v1
  accepted, so the narrowing is *proven* rather than asserted.
- **Both versions stay machine-checked** in `conformance/run.py`:
  `GOVERNED_WORK_ENUMS` pins v1's list as the §16.6 transcription and v2's as
  this amendment's, so neither can drift and v2 cannot be widened back.

C2 and K2 freeze to v2. Kovee's own `governance_show/enable/disable` result
schemas narrow to match under registry revision `k2-4`
(`kovee/spec/registry-README.md`), and `kovee-byom`'s `GOVERNANCE_OWNERS`
constant carries two arms.

The compatibility bundle string `byom_governed_work_v1` **does not move**.
Amendment A2 added an entire saga to the frozen bundle without renaming it,
and kovee amendment A1 replaced the bundle's underlying ontology and recorded
that `governed_work_binding_v1` "keeps its name". A record-schema successor is
not a new bundle.

## A9.3 — §25 is withdrawn

§25 ("Migration from Sage") specified `GovernanceCutover`, a fenced machine
whose whole purpose was to move a scope off the withdrawn arm. With no source
arm there is no cutover, and the section is withdrawn rather than left
standing as a machine nobody can build:

- `GovernanceCutover` is not implemented and not reserved. There is no
  cutover row, descriptor, operation, or state.
- `KoveeGovernanceOwnerBinding.cutover_ref` remains an optional member of the
  closed shape, unset by every machine in the stack. It is kept so a future
  *governed re-owning* transition — should one ever be specified — records its
  authority in a member that already exists, instead of widening a closed
  record under time pressure.
- **Greenfield enablement (`none → byom`) is the only owner transition.**
  Getting a scope back to `none` is `governance_disable`, which freezes the
  row; re-enablement is a fresh saga under a new binding epoch, never a
  reverse cutover.

The predecessor design is not a migration source, a compatibility target, or
a supported import path. Evidence that predates byom enters the way any other
foreign evidence does — as source-qualified inert `LegacyEvidence` under §7.5,
which manufactures no authority, assent, or Manifestation compatibility. That
general rule already exists and does not need a dedicated section.

## Follow-through

**Landed.** `DESIGN.md` §16.6's field block and §25 were re-cut in
**design-v0.2.1** (2026-07-27): the field block reads `byom | none` and §25
records the withdrawal instead of specifying `GovernanceCutover`. The ratified
byte-frozen v0.2 (sha256 `ccea384f…`, repo `cc4249c`) is unchanged and is what
the implementation plan still pins, so this record continues to control for
readers of that text. The machine-checked artifacts (the v2 schema, its
vectors, the runner's enum pins, and `GreenfieldEnablement.tla`'s type domain)
remain the operative statement of the rule.
