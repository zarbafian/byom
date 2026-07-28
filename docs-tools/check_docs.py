#!/usr/bin/env python3
"""Hold docs/ to the specs, the models and the code, so the site cannot rot.

A documentation site fails quietly. The source changes, the prose does not,
and nobody notices because nothing goes red. This script makes that failure
loud. Byom has already been bitten: a single sweep this week corrected the
registry from 83 rows to 99, the operation count from 73 to 76, the descriptor
machines from 20 to 26, and a TLC row reported as 101 states / depth 14 when
the model actually explores 102 / 15.

Four jobs, run in order:

  1. THE GENERATED REFERENCE. `docs-tools/gen_reference.py --check` proves
     docs/reference/index.html is exactly what the sources produce today.

  2. GENERATED BLOCKS. Every claim that is mechanically derivable lives inside
     a marked region of the HTML:

         <!--gen:registry-rows-->99<!--/gen:registry-rows-->

     `--write` fills those regions from the sources of truth. With no flag the
     script regenerates them in memory and compares — a divergence is a failure
     naming the block, the file, the expected text and the text on the page.
     Hand-editing a generated region fails exactly as loudly as source drift.

  3. FREE CLAIMS. Facts that read better inside a sentence than inside a block
     are asserted by presence: the script computes the truth and greps the
     pages for it. A second list is asserted by ABSENCE — retired facts and
     overclaims that must never reappear on any page.

  4. LINKS AND STRUCTURE. Every internal href/src resolves (including
     fragments), the sitemap lists exactly the index pages, every page is
     balanced HTML, and no page reaches out to a third-party host.

Sources of truth, in the order the script trusts them:

  spec/registry.json                the frozen (operation, surface) registry
  spec/schemas/**.json              the closed schemas, surfaces, problem kinds
  spec/descriptors/*.json           the transition machines
  spec/vectors/**.json              the golden and negative vectors
  spec/governed-work/*.json         the C2 record schemas
  spec/adr/README.md                the ADR index
  proof/PROPERTIES.md               the TLC results, re-run and recorded
  proof/specs/*.tla                 the models and their @parity blocks
  proof/negative-checks.py          the mutation suite's size
  family-vectors/                   the C1 profile corpus and its rederiver
  mcp/byom-mcp.tools.json           the MCP tool bundle
  design/*amendment*.md             the amendment record A1-A9
  conformance/i1-governed-loop/…    the I1 gate's own coverage verdict
  crates/bpp-core/src/limits.rs     the advertised wire limits
  crates/byomd/src/reads.rs         which operations this daemon serves
  Cargo.toml                        the workspace version

Usage:  python3 docs-tools/check_docs.py            # check; non-zero on drift
        python3 docs-tools/check_docs.py --write    # regenerate the blocks
"""

from __future__ import annotations

import html
import json
import pathlib
import re
import subprocess
import sys
from dataclasses import dataclass, field
from html.parser import HTMLParser

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"

# Tags that never close in HTML5, so a balance check must not wait for them.
VOID = {
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
}

NUMBER_WORDS = {
    0: "zero", 1: "one", 2: "two", 3: "three", 4: "four", 5: "five",
    6: "six", 7: "seven", 8: "eight", 9: "nine", 10: "ten", 11: "eleven",
    12: "twelve",
}


def die(msg: str) -> None:
    print(f"check-docs: FATAL — {msg}", file=sys.stderr)
    sys.exit(2)


def read(path: pathlib.Path) -> str:
    return path.read_text(encoding="utf-8")


def rel(p: pathlib.Path) -> str:
    return str(p.relative_to(ROOT))


def word(n: int) -> str:
    return NUMBER_WORDS.get(n, f"{n:,}")


def e(s) -> str:
    return html.escape(str(s))


def code_list(items, conjunction: str = "and") -> str:
    tagged = [f"<code>{e(i)}</code>" for i in items]
    if len(tagged) == 1:
        return tagged[0]
    return ", ".join(tagged[:-1]) + f" {conjunction} " + tagged[-1]


# --------------------------------------------------------------------------
# Reading the sources of truth
# --------------------------------------------------------------------------


def registry_rows() -> list[dict]:
    rows = json.loads(read(ROOT / "spec" / "registry.json"))["operations"]
    if len(rows) < 50:
        die(f"parsed only {len(rows)} registry rows — the registry moved")
    return rows


def registry_ops() -> set[str]:
    return {r["operation"] for r in registry_rows()}


def surface_enum() -> list[str]:
    schema = json.loads(read(ROOT / "spec" / "schemas" / "hello-result.schema.json"))
    return list(schema["properties"]["surface"]["enum"])


def socket_surfaces() -> list[str]:
    """The surfaces byomd actually binds a socket for, from SocketSurface::ALL."""
    src = read(ROOT / "crates" / "byomd" / "src" / "socket.rs")
    body = re.search(r"pub fn name\(self\) -> &'static str \{(.*?)\n    \}", src, re.S)
    if not body:
        die("SocketSurface::name not found in crates/byomd/src/socket.rs")
    names = re.findall(r'=> "([a-z_]+)"', body.group(1))
    if len(names) < 3:
        die(f"parsed only {len(names)} socket surfaces — the parser is wrong")
    return names


def problem_kinds() -> list[str]:
    schema = json.loads(read(ROOT / "spec" / "schemas" / "bpp-failure.schema.json"))
    found: list[list[str]] = []

    def walk(node) -> None:
        if isinstance(node, dict):
            enum = node.get("enum")
            if isinstance(enum, list) and "invalid" in enum and "internal" in enum:
                found.append(enum)
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(schema)
    if not found:
        die("no problem-kind enum in bpp-failure.schema.json")
    return list(found[0])


def descriptor_machines() -> list[dict]:
    out = []
    for path in sorted((ROOT / "spec" / "descriptors").glob("*.json")):
        if path.name == "vocabulary.json":
            continue
        machine = json.loads(read(path))
        machine["_file"] = path.name
        out.append(machine)
    if not out:
        die("no descriptors found")
    return out


def modeled_descriptor_files() -> dict[str, str]:
    out: dict[str, str] = {}
    for path in sorted((ROOT / "proof" / "specs").glob("*.tla")):
        for line in read(path).splitlines():
            m = re.match(r"\\\*\s*@parity\s+descriptors?:\s*(\S+\.json)\s*$", line.strip())
            if m:
                out[m.group(1)] = path.stem
    if len(out) < 8:
        die(f"parsed only {len(out)} @parity descriptor bindings — the parser is wrong")
    return out


def tla_modules() -> list[str]:
    mods = sorted(p.stem for p in (ROOT / "proof" / "specs").glob("*.tla"))
    if not mods:
        die("no TLA+ modules under proof/specs/")
    return mods


def tlc_results() -> list[tuple[str, str, str, str]]:
    """(module, distinct states, states generated, graph depth) from PROPERTIES.md.

    PROPERTIES.md records the numbers TLC last printed, and the ADR requires
    the coverage statement to live there. Parsing it means the site cannot
    quote a state count the proof README does not.
    """
    text = read(ROOT / "proof" / "PROPERTIES.md")
    rows = re.findall(
        r"^\|\s*`specs/([A-Za-z]+)\.tla`[^|]*\|\s*([\d,]+)\s*\|\s*([\d,]+)\s*\|\s*(\d+)\s*\|",
        text,
        re.M,
    )
    if len(rows) < 8:
        die(f"parsed only {len(rows)} TLC result rows from proof/PROPERTIES.md")
    known = set(tla_modules())
    for name, *_ in rows:
        if name not in known:
            die(f"proof/PROPERTIES.md reports on `{name}`, which is not a module "
                f"under proof/specs/")
    return rows


def parity_binding() -> dict[str, int]:
    """Modules, descriptors, states and transitions the parity gate agrees on.

    Recomputed here the way `proof/check-descriptors.py` computes it: the
    union of the `@parity state:` and `@parity transition:` lines, over the
    modules that declare a descriptor.
    """
    modules = 0
    descriptors: set[str] = set()
    states = 0
    transitions = 0
    for path in sorted((ROOT / "proof" / "specs").glob("*.tla")):
        text = read(path)
        if not re.search(r"^\\\*\s*@parity\s+module:", text, re.M):
            continue
        modules += 1
        # A module may declare `@parity none` — it is bound to the gate, it
        # just has no committed descriptor to be bound to.
        descriptors |= set(re.findall(r"@parity\s+descriptors?:\s*(\S+\.json)", text))
        states += len(re.findall(r"^\\\*\s*@parity\s+state:", text, re.M))
        transitions += len(re.findall(r"^\\\*\s*@parity\s+transition:", text, re.M))
    if not transitions:
        die("parsed no @parity transition lines — the parser is wrong")
    return {
        "modules": modules,
        "descriptors": len(descriptors),
        "states": states,
        "transitions": transitions,
    }


def machine_walks() -> int:
    """Executable state-walk vectors: the machine walks plus the C2 saga walks."""
    machines = list((ROOT / "spec" / "vectors" / "machines").glob("*.json"))
    sagas = [
        p for p in (ROOT / "spec" / "vectors" / "governed-work").glob("*.json")
        if isinstance(json.loads(read(p)).get("input", {}).get("steps"), list)
    ]
    if not machines or not sagas:
        die("no machine walk vectors found")
    return len(machines) + len(sagas)


def negative_mutations() -> int:
    src = read(ROOT / "proof" / "negative-checks.py")
    n = len(re.findall(r"^\s{4}report\(", src, re.M))
    if n < 5:
        die(f"parsed only {n} mutations in proof/negative-checks.py")
    return n


def family_vectors() -> dict[str, int]:
    """Files and cases per family, counted the way xcheck.py counts them."""
    root = ROOT / "family-vectors"
    families = sorted(
        d.name for d in root.iterdir()
        if d.is_dir() and not d.name.startswith(("_", ".")) and d.name != "tscheck"
        and any(d.glob("*.json"))
    )
    out: dict[str, int] = {}
    files = 0
    for fam in families:
        n = 0
        paths = sorted((root / fam).glob("*.json"))
        for p in paths:
            vec = json.loads(read(p))
            n += len(vec["cases"]) if isinstance(vec.get("cases"), list) else 1
        out[fam] = n
        files += len(paths)
    if not out:
        die("no family-vector families found")
    out["_files"] = files
    return out


def digest_classes() -> dict[str, str]:
    src = read(ROOT / "family-vectors" / "xcheck.py")
    m = re.search(r"DIGEST_CLASS_ALGORITHM = \{(.*?)\}", src, re.S)
    if not m:
        die("DIGEST_CLASS_ALGORITHM not found in family-vectors/xcheck.py")
    pairs = re.findall(r'"([a-z_]+)":\s*"([a-z0-9-]+)"', m.group(1))
    if len(pairs) < 4:
        die(f"parsed only {len(pairs)} digest classes — the parser is wrong")
    return dict(pairs)


def limits() -> dict[str, int]:
    src = read(ROOT / "crates" / "bpp-core" / "src" / "limits.rs")
    out = {
        name: int(value.replace("_", ""))
        for name, value in re.findall(
            r"pub const ([A-Z_]+): (?:usize|u64) = ([0-9_]+);", src
        )
    }
    for need in ("REQUEST_MAX_BYTES", "RESPONSE_MAX_BYTES", "JSON_DEPTH_MAX",
                 "JSON_NODES_MAX", "IDENTIFIER_MAX_BYTES", "EVENTS_PAGE_ITEMS_MAX"):
        if need not in out:
            die(f"{need} not found in crates/bpp-core/src/limits.rs")
    return out


def served_ops() -> set[str]:
    src = read(ROOT / "crates" / "byomd" / "src" / "reads.rs")
    body = re.search(r"pub fn implemented\(op: &str\) -> bool \{(.*?)\n\}", src, re.S)
    if not body:
        die("reads::implemented not found in crates/byomd/src/reads.rs")
    consts = re.findall(r"([A-Z0-9_]+)\.contains", body.group(1))
    out: set[str] = set(re.findall(r'op == "([a-z0-9_]+)"', body.group(1)))
    if not consts:
        die("reads::implemented names no const op lists — the parser is wrong")
    for const in consts:
        m = re.search(rf"const {const}:[^=]*=\s*\[(.*?)\];", src, re.S)
        if not m:
            die(f"{const} is used by implemented() but is not declared in reads.rs")
        out |= set(re.findall(r'"([a-z0-9_]+)"', m.group(1)))
    return out


def mcp_document() -> dict:
    return json.loads(read(ROOT / "mcp" / "byom-mcp.tools.json"))


def workspace_version() -> str:
    src = read(ROOT / "Cargo.toml")
    m = re.search(r'\[workspace\.package\][^\[]*?version\s*=\s*"([^"]+)"', src, re.S)
    if not m:
        die("no version in [workspace.package]")
    return m.group(1)


def protocol_version() -> str:
    src = read(ROOT / "crates" / "bpp-core" / "src" / "lib.rs")
    m = re.search(r'PROTOCOL_VERSION: &str = "([0-9.]+)"', src)
    if not m:
        die("PROTOCOL_VERSION not found in crates/bpp-core/src/lib.rs")
    return m.group(1)


def i1_coverage() -> dict[str, int]:
    items = json.loads(
        read(ROOT / "conformance" / "i1-governed-loop" / "evidence"
             / "all-checks-coverage.json")
    )
    exercised = sum(1 for i in items if i["status"] == "exercised")
    simulated = sum(1 for i in items if i["status"] == "SIMULATED")
    if exercised + simulated != len(items):
        die("the I1 coverage file has a status this script does not know: "
            + ", ".join(sorted({i["status"] for i in items})))
    return {"total": len(items), "exercised": exercised, "simulated": simulated}


def amendments() -> list[tuple[str, str, str]]:
    """(id, title, status) for every amendment record in design/.

    The amendment files are digest-pinned by the family lock, so this reads
    them and never writes them.
    """
    out: list[tuple[str, str, str]] = []
    for path in sorted((ROOT / "design").glob("*amendment*.md")):
        text = read(path)
        status = "recorded"
        m = re.search(r"^Status:\s*(.+)$", text, re.M)
        if m:
            status = m.group(1).strip()
        for hid, title in re.findall(r"^## (A\d)\s*—\s*(.+)$", text, re.M):
            out.append((hid, title.strip(), status))
        for hid, title in re.findall(r"^# Amendment (A\d):\s*(.+)$", text, re.M):
            out.append((hid, title.strip(), status))
    out.sort(key=lambda r: int(r[0][1:]))
    if len(out) < 8:
        die(f"parsed only {len(out)} amendments from design/ — the parser is wrong")
    return out


def adrs() -> list[tuple[str, str, str, str]]:
    """(number, plan id, title, status) from the ADR index table."""
    text = read(ROOT / "spec" / "adr" / "README.md")
    rows = re.findall(
        r"^\|\s*\[(\d+)\]\(([^)]+)\)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|",
        text,
        re.M,
    )
    if not rows:
        die("no ADR rows parsed from spec/adr/README.md")
    return [(n, plan.strip(), title.strip(), status.strip())
            for n, _href, plan, title, status in rows]


def count_json(*parts: str) -> int:
    root = ROOT.joinpath(*parts)
    n = len(list(root.rglob("*.json")))
    if n == 0:
        die(f"no JSON files under {'/'.join(parts)}")
    return n


# --------------------------------------------------------------------------
# The generated blocks
# --------------------------------------------------------------------------


def gen_version() -> str:
    return e(workspace_version())


def gen_protocol_version() -> str:
    return e(protocol_version())


def gen_registry_rows() -> str:
    return str(len(registry_rows()))


def gen_registry_ops() -> str:
    return str(len(registry_ops()))


def gen_registry_families() -> str:
    return str(len({r["family"] for r in registry_rows()}))


def gen_socket_count() -> str:
    return word(len(socket_surfaces()))


def gen_socket_surfaces() -> str:
    return code_list([f"{s}.sock" for s in socket_surfaces()])


def gen_surface_enum_count() -> str:
    return word(len(surface_enum()))


def gen_surface_enum() -> str:
    return code_list(surface_enum())


def gen_admin_rows() -> str:
    return word(sum(1 for r in registry_rows() if r["surface"] == "admin"))


def gen_served_count() -> str:
    return str(len(served_ops() & registry_ops()))


def gen_unserved_count() -> str:
    return word(len(registry_ops() - served_ops()))


def gen_unserved_ops() -> str:
    return code_list(sorted(registry_ops() - served_ops()))


def gen_op_class_counts() -> str:
    rows = registry_rows()
    parts = []
    for cls in ("create", "update", "read"):
        n = sum(1 for r in rows if r["class"] == cls)
        parts.append(f"{n} <code>{cls}</code>")
    return ", ".join(parts[:-1]) + ", and " + parts[-1]


def gen_schema_count() -> str:
    """Every published JSON Schema the conformance runner compiles.

    The envelope and per-operation suite under `spec/schemas/`, plus the C2
    record schemas under `spec/governed-work/` — the runner compiles both
    trees and reports one total.
    """
    schemas = len(list((ROOT / "spec" / "schemas").rglob("*.schema.json")))
    records = len(list((ROOT / "spec" / "governed-work").glob("*.schema.json")))
    if not schemas or not records:
        die("no schemas found under spec/schemas/ or spec/governed-work/")
    return str(schemas + records)


def gen_vector_count() -> str:
    return str(count_json("spec", "vectors"))


def gen_governed_work_schemas() -> str:
    return str(count_json("spec", "governed-work"))


def gen_descriptor_count() -> str:
    return str(len(descriptor_machines()))


def gen_descriptor_states() -> str:
    return str(sum(len(m.get("states", [])) for m in descriptor_machines()))


def gen_descriptor_transitions() -> str:
    return str(sum(len(m.get("transitions", [])) for m in descriptor_machines()))


def gen_descriptor_kovee_owned() -> str:
    return word(sum(1 for m in descriptor_machines() if m.get("owner", "byom") != "byom"))


def gen_descriptor_unmodeled() -> str:
    modeled = set(modeled_descriptor_files())
    return str(sum(1 for m in descriptor_machines() if m["_file"] not in modeled))


def gen_b01_mutating() -> str:
    """Mutating operations on the B0.1 sheet — the set the one-to-one rule ranges over."""
    rows = registry_rows()
    ops = {r["operation"] for r in rows
           if r.get("bundle", "B0.1") == "B0.1" and r["class"] != "read"}
    return str(len(ops))


def gen_model_count() -> str:
    return word(len(tla_modules()))


def gen_tla_modules() -> str:
    return code_list([f"{m}.tla" for m in tla_modules()])


def gen_model_rows() -> str:
    rows = []
    for name, distinct, generated, depth in tlc_results():
        rows.append(
            f"  <tr><td><code>{e(name)}.tla</code></td>"
            f'<td class="num">{e(distinct)}</td>'
            f'<td class="num">{e(generated)}</td>'
            f'<td class="num">{e(depth)}</td></tr>'
        )
    return "\n" + "\n".join(rows) + "\n"


def gen_parity_summary() -> str:
    p = parity_binding()
    return (f"{p['modules']} modules, {p['descriptors']} descriptors, "
            f"{p['states']} states and {p['transitions']} transitions in exact "
            f"agreement in both directions")


def gen_machine_walk_count() -> str:
    return str(machine_walks())


def gen_negative_mutation_count() -> str:
    return str(negative_mutations())


def gen_family_vector_files() -> str:
    return str(family_vectors()["_files"])


def gen_family_vector_cases() -> str:
    fv = family_vectors()
    return str(sum(v for k, v in fv.items() if k != "_files"))


def gen_family_vector_family_rows() -> str:
    fv = family_vectors()
    rows = []
    for fam in sorted(k for k in fv if k != "_files"):
        why = FAMILY_PURPOSE.get(fam)
        if why is None:
            die(f"family-vectors/{fam}/ exists but this script has no description "
                f"for it — add one to FAMILY_PURPOSE in docs-tools/check_docs.py")
        rows.append(f'  <tr><td><code>{e(fam)}/</code></td>'
                    f'<td class="num">{fv[fam]}</td><td>{why}</td></tr>')
    return "\n" + "\n".join(rows) + "\n"


def gen_digest_class_count() -> str:
    return word(len(digest_classes()))


def gen_digest_class_rows() -> str:
    rows = []
    for name, algorithm in digest_classes().items():
        why = DIGEST_CLASS_USE.get(name)
        if why is None:
            die(f"the profile declares digest class `{name}` but this script has "
                f"no description for it — add one to DIGEST_CLASS_USE")
        keyed = "required" if algorithm == "hmac-sha-256" else "forbidden"
        rows.append(f"  <tr><td><code>{e(name)}</code></td>"
                    f"<td><code>{e(algorithm)}</code></td><td>{keyed}</td>"
                    f"<td>{why}</td></tr>")
    return "\n" + "\n".join(rows) + "\n"


def gen_problem_kind_count() -> str:
    return str(len(problem_kinds()))


def gen_problem_kinds() -> str:
    return ", ".join(f"<code>{e(k)}</code>" for k in problem_kinds())


def gen_mcp_candidate_tools() -> str:
    return word(len(mcp_document()["profiles"]["candidate"]["tools"]))


def gen_mcp_participant_tools() -> str:
    return str(len(mcp_document()["profiles"]["participant"]["tools"]))


def gen_mcp_doc_version() -> str:
    return e(mcp_document()["version"])


def gen_limit_request_bytes() -> str:
    return f"{limits()['REQUEST_MAX_BYTES']:,}"


def gen_limit_response_bytes() -> str:
    return f"{limits()['RESPONSE_MAX_BYTES']:,}"


def gen_limit_depth() -> str:
    return str(limits()["JSON_DEPTH_MAX"])


def gen_limit_nodes() -> str:
    return f"{limits()['JSON_NODES_MAX']:,}"


def gen_limit_identifier_bytes() -> str:
    return str(limits()["IDENTIFIER_MAX_BYTES"])


def gen_limit_events_page() -> str:
    return str(limits()["EVENTS_PAGE_ITEMS_MAX"])


def gen_i1_total() -> str:
    return str(i1_coverage()["total"])


def gen_i1_exercised() -> str:
    return str(i1_coverage()["exercised"])


def gen_i1_simulated() -> str:
    return str(i1_coverage()["simulated"])


def gen_amendment_rows() -> str:
    rows = []
    for hid, title, status in amendments():
        why = AMENDMENT_WHY.get(hid)
        if why is None:
            die(f"design/ records amendment {hid} but this script has no summary "
                f"for it — add one to AMENDMENT_WHY in docs-tools/check_docs.py")
        # Titles are verbatim from the amendment headings, which are markdown:
        # render their backticked spans as code rather than as literal ticks.
        marked = re.sub(r"`([^`]+)`", r"<code>\1</code>", e(title))
        rows.append(f"  <tr><td><code>{e(hid)}</code></td><td>{marked}</td>"
                    f"<td>{why}</td><td>{e(status)}</td></tr>")
    return "\n" + "\n".join(rows) + "\n"


def gen_adr_rows() -> str:
    rows = []
    for number, plan, title, status in adrs():
        rows.append(f"  <tr><td><code>ADR-{e(number)}</code></td>"
                    f"<td><code>{e(plan)}</code></td><td>{e(title)}</td>"
                    f"<td>{e(status)}</td></tr>")
    return "\n" + "\n".join(rows) + "\n"


# --------------------------------------------------------------------------
# The prose the generator owns
# --------------------------------------------------------------------------
#
# Names and counts come from the sources. The one-line purposes are editorial,
# so they live here — keyed by the name the source produces. A new digest
# class, family or amendment therefore fails this script with "no description",
# which is the point: the sources cannot grow without the site being told.

FAMILY_PURPOSE = {
    "ijson": "Strict I-JSON acceptance: the ordered check list, its error classes, and the caps.",
    "jcs": "RFC 8785 canonical bytes, including UTF-16 key sorting and ECMAScript number form.",
    "problem": "The RFC 9457 problem shape, its closed kind enum, and the exact <code>type</code> prefix.",
    "idempotency": "The idempotency-domain derivations, byom's and kovee's side by side, pinned as distinct.",
    "digest-class": "The six classes, their wire shape, and the complete ordered substitution matrix.",
    "privacy": "The <code>PrivacyAccessRecord</code> preimage and the chain link, including genesis absence.",
}

DIGEST_CLASS_USE = {
    "structural_public": (
        "Knowingly non-sensitive, non-erasable protocol or schema bytes only."
    ),
    "portable_public": (
        "Content whose owner explicitly accepted a durable, publicly "
        "dictionary-testable identifier — and the class the A8 rule demands of "
        "any digest one protocol asks the other for."
    ),
    "local_erasure_safe": (
        "Ordinary erasable local content and every authority subject, under a "
        "random <strong>per-object</strong> secret."
    ),
    "scope_erasure_safe": (
        "Shared-key index and chain constructions, under a "
        "<strong>per-scope</strong> key."
    ),
    "disclosed_party": (
        "Bytes already disclosed to named recipients, always carrying the "
        "external-copy obligation."
    ),
    "ciphertext_public": (
        "Sealed blob bytes only — never a commitment to low-entropy plaintext."
    ),
}

AMENDMENT_WHY = {
    "A1": "The gateway is <strong>akson</strong>; the historical path name is retired.",
    "A2": (
        "Kovee integration is greenfield-first: byom is kovee's governance owner "
        "from day one, there is no migration path in, and the greenfield "
        "enablement saga is added to the governed-work bundle."
    ),
    "A3": "Adds the two review cadences that make B0.1/C3a and C4 implementation-ready.",
    "A4": (
        "Adds the <strong>candidate</strong> MCP profile: one offer, three tools, "
        "closed server-side the moment the offer terminates. Elicitation through it "
        "is never assent."
    ),
    "A5": (
        "Typed profile-claim publish/read/search operations are a tracked "
        "obligation, not a shipped capability — until they exist no ranked-routing "
        "claim is advertised."
    ),
    "A6": (
        "Adds an honestly weaker <strong>manual developer profile</strong> for "
        "sovereign exchange, with no execution evidence claimed."
    ),
    "A7": "Re-slices B1 into an attached (I0) and a hosted (I1) slice with separate exits.",
    "A8": (
        "The cross-boundary digest-class rule, and the corrected activation order. "
        "Found by two independent implementations meeting at a live seam, not by "
        "review."
    ),
    "A9": (
        "Narrows the governance-owner enum to <code>byom | none</code> and "
        "withdraws the cutover machine, leaving greenfield enablement as the only "
        "owner transition in the stack."
    ),
}


BLOCKS = {
    "version": gen_version,
    "protocol-version": gen_protocol_version,
    "registry-rows": gen_registry_rows,
    "registry-ops": gen_registry_ops,
    "registry-families": gen_registry_families,
    "socket-count": gen_socket_count,
    "socket-surfaces": gen_socket_surfaces,
    "surface-enum-count": gen_surface_enum_count,
    "surface-enum": gen_surface_enum,
    "admin-rows": gen_admin_rows,
    "served-count": gen_served_count,
    "unserved-count": gen_unserved_count,
    "unserved-ops": gen_unserved_ops,
    "op-class-counts": gen_op_class_counts,
    "schema-count": gen_schema_count,
    "vector-count": gen_vector_count,
    "governed-work-schemas": gen_governed_work_schemas,
    "descriptor-count": gen_descriptor_count,
    "descriptor-states": gen_descriptor_states,
    "descriptor-transitions": gen_descriptor_transitions,
    "descriptor-kovee-owned": gen_descriptor_kovee_owned,
    "descriptor-unmodeled": gen_descriptor_unmodeled,
    "b01-mutating": gen_b01_mutating,
    "model-count": gen_model_count,
    "tla-modules": gen_tla_modules,
    "model-rows": gen_model_rows,
    "parity-summary": gen_parity_summary,
    "machine-walk-count": gen_machine_walk_count,
    "negative-mutation-count": gen_negative_mutation_count,
    "family-vector-files": gen_family_vector_files,
    "family-vector-cases": gen_family_vector_cases,
    "family-vector-family-rows": gen_family_vector_family_rows,
    "digest-class-count": gen_digest_class_count,
    "digest-class-rows": gen_digest_class_rows,
    "problem-kind-count": gen_problem_kind_count,
    "problem-kinds": gen_problem_kinds,
    "mcp-candidate-tools": gen_mcp_candidate_tools,
    "mcp-participant-tools": gen_mcp_participant_tools,
    "mcp-doc-version": gen_mcp_doc_version,
    "limit-request-bytes": gen_limit_request_bytes,
    "limit-response-bytes": gen_limit_response_bytes,
    "limit-depth": gen_limit_depth,
    "limit-nodes": gen_limit_nodes,
    "limit-identifier-bytes": gen_limit_identifier_bytes,
    "limit-events-page": gen_limit_events_page,
    "i1-total": gen_i1_total,
    "i1-exercised": gen_i1_exercised,
    "i1-simulated": gen_i1_simulated,
    "amendment-rows": gen_amendment_rows,
    "adr-rows": gen_adr_rows,
}

BLOCK_RE = re.compile(r"<!--gen:([a-z0-9-]+)-->(.*?)<!--/gen:\1-->", re.S)


# --------------------------------------------------------------------------
# Checks
# --------------------------------------------------------------------------


@dataclass
class Report:
    failures: list[str] = field(default_factory=list)
    checked: int = 0

    def fail(self, msg: str) -> None:
        self.failures.append(msg)

    def ok(self, n: int = 1) -> None:
        self.checked += n


def pages() -> list[pathlib.Path]:
    return sorted(DOCS.rglob("*.html"))


def check_reference(rep: Report) -> None:
    proc = subprocess.run(
        [sys.executable, str(ROOT / "docs-tools" / "gen_reference.py"), "--check"],
        capture_output=True,
        text=True,
    )
    sys.stdout.write(proc.stdout)
    if proc.returncode != 0:
        rep.fail("the generated reference page is stale:\n        "
                 + (proc.stderr.strip() or "gen_reference.py --check failed"))
    else:
        rep.ok()


def check_blocks(rep: Report, write: bool) -> None:
    seen: set[str] = set()
    for page in pages():
        text = read(page)
        changed = False

        def sub(m: re.Match) -> str:
            nonlocal changed
            name, current = m.group(1), m.group(2)
            seen.add(name)
            gen = BLOCKS.get(name)
            if gen is None:
                rep.fail(
                    f"{rel(page)}: <!--gen:{name}--> is not a block this script "
                    f"knows how to generate (known: {', '.join(sorted(BLOCKS))})"
                )
                return m.group(0)
            want = gen()
            rep.ok()
            if want != current:
                if write:
                    changed = True
                    return f"<!--gen:{name}-->{want}<!--/gen:{name}-->"
                rep.fail(
                    f"{rel(page)}: generated block `{name}` is STALE.\n"
                    f"        the page says:   {current.strip()[:300]}\n"
                    f"        the source says: {want.strip()[:300]}\n"
                    f"        fix: python3 docs-tools/check_docs.py --write"
                )
            return m.group(0)

        new = BLOCK_RE.sub(sub, text)
        if write and changed:
            page.write_text(new, encoding="utf-8")
            print(f"  rewrote {rel(page)}")

    unused = set(BLOCKS) - seen
    if unused:
        rep.fail(
            "these generated blocks exist in this script but appear on no page, so "
            "nothing is being held to them: " + ", ".join(sorted(unused))
        )


def check_free_claims(rep: Report) -> None:
    """Facts that read better in a sentence: assert by presence, and by absence."""
    index = DOCS / "index.html"
    protocol = DOCS / "protocol" / "index.html"
    encoding = DOCS / "encoding" / "index.html"
    proofs = DOCS / "proofs" / "index.html"
    security = DOCS / "security" / "index.html"
    governed = DOCS / "governed-work" / "index.html"

    required: list[tuple[str, str, pathlib.Path]] = [
        ("the workspace version", workspace_version(), index),
        ("the workspace version", workspace_version(), security),
        ("the negotiated protocol version", protocol_version(), protocol),
        ("the problem type namespace", "https://byom.dev/problems/", encoding),
        ("the provisional schema namespace", "byom.example", security),
        ("the compatibility bundle string", "byom_governed_work_v1", governed),
    ]
    for surface in socket_surfaces():
        required.append((f"the socket surface `{surface}`", f"{surface}.sock", protocol))
    for name in digest_classes():
        required.append((f"the digest class `{name}`", name, encoding))
    for module in tla_modules():
        required.append((f"the TLA+ module `{module}`", module, proofs))
    for fam in family_vectors():
        if fam != "_files":
            required.append((f"the family-vector family `{fam}`", fam, encoding))
    # The frozen cross-boundary digest domains: a counterparty must derive
    # these, so they are wire-visible names and belong on the page.
    for domain in ("bpp-resource-allocation-binding-v0",
                   "bpp-mandate-use-binding-v0",
                   "bpp-execution-consumption-receipt-binding-v0"):
        required.append((f"the frozen cross-boundary domain `{domain}`", domain, encoding))
    # The single most important honest statement on the site. It must appear
    # wherever the proofs are described, not once in a footnote.
    for page in (index, proofs, security):
        required.append(
            ("the conformance-oracle gap",
             "reads a descriptor or a TLA+ model", page)
        )
    for page in (index, security):
        required.append(("the assurance profile", "developer", page))
        required.append(("the unsigned-receipt limit", "does not sign", page))
        required.append(("the designed exit code", "exit 2", page))

    # Prose wraps, so both needle and haystack are compared with runs of
    # whitespace squashed — otherwise a claim would "disappear" from a page
    # merely by being reflowed across a line break.
    def squash(s: str) -> str:
        return re.sub(r"\s+", " ", s)

    for what, needle, page in required:
        if not page.exists():
            rep.fail(f"{rel(page)}: missing, but {what} must appear on it")
            continue
        if squash(needle) not in squash(read(page)):
            rep.fail(
                f"{rel(page)}: {what} is {needle!r} in the source, and that string "
                f"does not appear on the page"
            )
        else:
            rep.ok()

    # Claims that must NEVER appear: retired facts and the overclaims this
    # program has spent thirty-odd corrections removing.
    forbidden: list[tuple[str, str]] = [
        (r"\bbpp-spec\b",
         "there has never been a `bpp-spec` crate — ADR-0003's erratum records "
         "that the command it names cannot run"),
        (r"spec/models/",
         "there is no spec/models/ directory; the models live under proof/specs/"),
        (r"(?i)\bsage\b",
         "byom replaces the discarded predecessor design; amendment A9 withdrew "
         "its enum arm, and no outward material carries its lineage"),
        (r"(?i)(?<!not )\bproduction[- ]ready\b",
         "byom is pre-release; the phrase may only appear negated"),
        (r"(?i)\bmodels?\s+(?:are|is)\s+tied\s+to\s+the\s+(?:code|implementation)",
         "no automated test connects the models to the implementation — saying "
         "otherwise is precisely the overclaim ADR-0003's erratum corrects"),
        (r"(?i)\bformally\s+verified\b",
         "TLC explores a bounded state space at fixed constants; no model is "
         "proved for arbitrary run length"),
    ]
    for pattern, why in forbidden:
        rx = re.compile(pattern)
        for page in pages():
            if rx.search(squash(read(page))):
                rep.fail(f"{rel(page)}: matches /{pattern}/ — {why}")
            else:
                rep.ok()


def check_links(rep: Report) -> None:
    """Every internal href/src resolves to something in docs/, and fragments exist."""
    ids: dict[pathlib.Path, set[str]] = {}
    for page in pages():
        ids[page] = set(re.findall(r'\bid="([^"]+)"', read(page)))

    href_re = re.compile(r'\b(?:href|src)="([^"]*)"')
    for page in pages():
        for raw in href_re.findall(read(page)):
            link = html.unescape(raw)
            if not link or link.startswith(("http://", "https://", "mailto:", "data:", "//")):
                continue
            path_part, _, frag = link.partition("#")
            if not path_part:
                if frag and frag not in ids[page]:
                    rep.fail(f"{rel(page)}: #{frag} — no element with that id on this page")
                else:
                    rep.ok()
                continue
            base = DOCS if path_part.startswith("/") else page.parent
            target = (base / path_part.lstrip("/")).resolve()
            try:
                target.relative_to(DOCS.resolve())
            except ValueError:
                rep.fail(f"{rel(page)}: {link} escapes docs/")
                continue
            if target.is_dir():
                target = target / "index.html"
            if not target.exists():
                rep.fail(f"{rel(page)}: {link} -> {rel(target)} does not exist")
                continue
            if frag:
                if target.suffix != ".html":
                    rep.fail(f"{rel(page)}: {link} has a fragment but the target is not HTML")
                    continue
                target_ids = ids.get(target)
                if target_ids is None:
                    target_ids = set(re.findall(r'\bid="([^"]+)"', read(target)))
                if frag not in target_ids:
                    rep.fail(f"{rel(page)}: {link} — no element with id {frag!r} in the target")
                    continue
            rep.ok()

    # The sitemap must list exactly the indexable pages, and nothing that 404s.
    sitemap = DOCS / "sitemap.xml"
    listed = set(re.findall(r"<loc>https://byom\.cc/([^<]*)</loc>", read(sitemap)))
    on_disk = set()
    for page in pages():
        if page.name != "index.html":
            continue
        r = page.relative_to(DOCS).parent.as_posix()
        on_disk.add("" if r == "." else r + "/")
    if listed != on_disk:
        rep.fail(
            "docs/sitemap.xml does not list exactly the site's index pages.\n"
            f"        only in sitemap: {sorted(listed - on_disk) or 'none'}\n"
            f"        only on disk:    {sorted(on_disk - listed) or 'none'}"
        )
    else:
        rep.ok()

    # The custom domain: without this file in the published source, the Pages
    # deploy can drop the byom.cc setting.
    cname = DOCS / "CNAME"
    if not cname.exists() or cname.read_text().strip() != "byom.cc":
        rep.fail("docs/CNAME must exist and contain exactly `byom.cc`")
    else:
        rep.ok()
    if not (DOCS / ".nojekyll").exists():
        rep.fail("docs/.nojekyll is missing — Pages would run the file tree "
                 "through Jekyll")
    else:
        rep.ok()


class Balance(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.stack: list[tuple[str, int]] = []
        self.errors: list[str] = []

    def handle_starttag(self, tag, attrs):
        if tag not in VOID:
            self.stack.append((tag, self.getpos()[0]))

    def handle_startendtag(self, tag, attrs):
        pass

    def handle_endtag(self, tag):
        if tag in VOID:
            return
        if not self.stack:
            self.errors.append(f"line {self.getpos()[0]}: </{tag}> with nothing open")
            return
        if self.stack[-1][0] != tag:
            open_tag, line = self.stack[-1]
            self.errors.append(
                f"line {self.getpos()[0]}: </{tag}> closes <{open_tag}> opened on line {line}"
            )
            # Recover if it matches something further down, so one slip does not
            # cascade into a hundred bogus errors.
            for i in range(len(self.stack) - 1, -1, -1):
                if self.stack[i][0] == tag:
                    del self.stack[i:]
                    return
            return
        self.stack.pop()


def check_html(rep: Report) -> None:
    for page in pages():
        text = read(page)
        parser = Balance()
        parser.feed(text)
        parser.close()
        for err in parser.errors:
            rep.fail(f"{rel(page)}: {err}")
        for tag, line in parser.stack:
            rep.fail(f"{rel(page)}: <{tag}> opened on line {line} is never closed")
        if not parser.errors and not parser.stack:
            rep.ok()
        if "<title>" not in text:
            rep.fail(f"{rel(page)}: no <title>")
        else:
            rep.ok()
        if 'lang="en"' not in text:
            rep.fail(f"{rel(page)}: <html> carries no lang attribute")
        else:
            rep.ok()
        # Self-contained: a Pages host will happily serve a page that reaches
        # out to a CDN. This site must not.
        for m in re.finditer(r'<link\b[^>]*\bhref="(https?://[^"]+)"[^>]*>', text):
            if re.search(r'rel="(stylesheet|preconnect|preload|modulepreload)"', m.group(0)):
                rep.fail(f"{rel(page)}: external asset {m.group(1)}")
        if re.search(r"<script[^>]+\bsrc=\"https?://", text):
            rep.fail(f"{rel(page)}: external script")
        if re.search(r'<img[^>]+\bsrc="https?://', text):
            rep.fail(f"{rel(page)}: external image")
        if "@import" in text and "url(http" in text:
            rep.fail(f"{rel(page)}: external @import")


def main() -> int:
    write = "--write" in sys.argv[1:]
    rep = Report()

    print("docs: the generated reference page vs. the sources of truth")
    check_reference(rep)

    print("docs: generated blocks vs. the sources of truth")
    check_blocks(rep, write)
    if write:
        # A rewrite invalidates the counts above; re-check so the exit code means
        # "the tree is now consistent", not "it was consistent before I wrote".
        rep = Report()
        check_reference(rep)
        check_blocks(rep, False)

    print("docs: free claims, present and absent")
    check_free_claims(rep)

    print("docs: internal links, fragments, the sitemap, CNAME")
    check_links(rep)

    print("docs: HTML is balanced and self-contained")
    check_html(rep)

    print()
    if rep.failures:
        for f in rep.failures:
            print(f"  FAIL  {f}")
        print(f"\ndocs check: {len(rep.failures)} FAILED, {rep.checked} passed")
        return 1
    print(f"docs check: {rep.checked} claims and links verified against the sources")
    return 0


if __name__ == "__main__":
    sys.exit(main())
