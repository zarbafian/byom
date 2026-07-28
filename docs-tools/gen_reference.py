#!/usr/bin/env python3
"""Generate docs/reference/index.html from the pinned sources.

Nothing on the reference page is typed by hand. Every row, count, name and
column is read out of:

    spec/registry.json              the frozen (operation, surface) registry
    spec/schemas/hello-result…      the closed surface enum
    spec/schemas/bpp-failure…       the closed problem-kind enum
    spec/schemas/ops/*.json         the per-operation request/result suite
    spec/descriptors/*.json         the transition machines
    proof/specs/*.tla               which machines have a model
    family-vectors/xcheck.py        the six digest classes and their algorithms
    crates/bpp-core/src/limits.rs   the advertised byte/shape limits
    crates/byomd/src/reads.rs       which operations this daemon serves

Run it after any change to those files:

    python3 docs-tools/gen_reference.py            # write the page
    python3 docs-tools/gen_reference.py --check    # fail if the page is stale

`docs-tools/check_docs.py` runs --check, so a source change that never
reached the site turns run-checks.sh red.
"""

from __future__ import annotations

import html
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "reference" / "index.html"
GH = "https://github.com/zarbafian/byom/blob/main"


def die(msg: str) -> None:
    print(f"gen_reference: FATAL — {msg}", file=sys.stderr)
    sys.exit(2)


def read(path: pathlib.Path) -> str:
    return path.read_text(encoding="utf-8")


def e(s: str) -> str:
    return html.escape(str(s))


# ------------------------------------------------------------------ sources


def registry() -> list[dict]:
    rows = json.loads(read(ROOT / "spec" / "registry.json"))["operations"]
    if not rows:
        die("spec/registry.json has no operations")
    return rows


def surface_enum() -> list[str]:
    schema = json.loads(read(ROOT / "spec" / "schemas" / "hello-result.schema.json"))
    return list(schema["properties"]["surface"]["enum"])


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


def descriptors() -> list[dict]:
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


def modeled_descriptors() -> dict[str, str]:
    """descriptor file -> the TLA+ module that declares parity with it.

    The `@parity` block after each module terminator names its descriptor
    files; that block is the same one `proof/check-descriptors.py` enforces,
    so this mapping cannot drift from the parity gate without failing it.
    """
    out: dict[str, str] = {}
    for path in sorted((ROOT / "proof" / "specs").glob("*.tla")):
        for line in read(path).splitlines():
            m = re.match(r"\\\*\s*@parity\s+descriptors?:\s*(\S+\.json)\s*$", line.strip())
            if m:
                out[m.group(1)] = path.stem
    if len(out) < 8:
        die(f"parsed only {len(out)} @parity descriptor bindings — the parser is wrong")
    for name in out:
        if not (ROOT / "spec" / "descriptors" / name).exists():
            die(f"a model declares parity with spec/descriptors/{name}, which does "
                f"not exist")
    return out


def tla_modules() -> list[str]:
    return sorted(p.stem for p in (ROOT / "proof" / "specs").glob("*.tla"))


def digest_classes() -> dict[str, str]:
    src = read(ROOT / "family-vectors" / "xcheck.py")
    m = re.search(r"DIGEST_CLASS_ALGORITHM = \{(.*?)\}", src, re.S)
    if not m:
        die("DIGEST_CLASS_ALGORITHM not found in family-vectors/xcheck.py")
    pairs = re.findall(r'"([a-z_]+)":\s*"([a-z0-9-]+)"', m.group(1))
    if len(pairs) != 6:
        die(f"expected six digest classes, parsed {len(pairs)}")
    return dict(pairs)


def limits() -> dict[str, int]:
    src = read(ROOT / "crates" / "bpp-core" / "src" / "limits.rs")
    out: dict[str, int] = {}
    for name, value in re.findall(
        r"pub const ([A-Z_]+): (?:usize|u64) = ([0-9_]+);", src
    ):
        out[name] = int(value.replace("_", ""))
    for need in ("REQUEST_MAX_BYTES", "RESPONSE_MAX_BYTES", "JSON_DEPTH_MAX",
                 "JSON_NODES_MAX", "IDENTIFIER_MAX_BYTES",
                 "MUTATION_LIST_ITEMS_MAX", "EVENTS_PAGE_ITEMS_MAX"):
        if need not in out:
            die(f"{need} not found in crates/bpp-core/src/limits.rs")
    return out


def served_ops() -> set[str]:
    """The operations this daemon implements, from `reads::implemented`.

    `implemented()` is the union of const arrays; the registry rows it does
    not cover answer `feature_unavailable` rather than being absent.
    """
    src = read(ROOT / "crates" / "byomd" / "src" / "reads.rs")
    body = re.search(r"pub fn implemented\(op: &str\) -> bool \{(.*?)\n\}", src, re.S)
    if not body:
        die("reads::implemented not found in crates/byomd/src/reads.rs")
    consts = re.findall(r"([A-Z0-9_]+)\.contains", body.group(1))
    literals = re.findall(r'op == "([a-z0-9_]+)"', body.group(1))
    if not consts:
        die("reads::implemented names no const op lists — the parser is wrong")
    out: set[str] = set(literals)
    for const in consts:
        m = re.search(rf"const {const}:[^=]*=\s*\[(.*?)\];", src, re.S)
        if not m:
            die(f"{const} is used by implemented() but is not declared in reads.rs")
        out |= set(re.findall(r'"([a-z0-9_]+)"', m.group(1)))
    return out


# ------------------------------------------------------------------ editorial
#
# Names and counts come from the sources. The one-line purposes are editorial,
# so they live here — keyed by the name the source produces. A new surface or
# a new digest class therefore fails this script with "no description", which
# is the point: the registry cannot grow without the site being told.

SURFACE_PURPOSE = {
    "governance": (
        "Decisions made <em>about</em> a Society: bootstrap, charter, membership "
        "offers, admission, mandates, and the two reconciliation seats. The seat "
        "where authority is granted."
    ),
    "candidate": (
        "The only surface an unadmitted candidate can reach — offer-scoped, minted "
        "with one <code>MembershipOffer</code> and fenced the moment that offer "
        "terminates. It can accept, refuse, or propose its own self-policy, and "
        "nothing else."
    ),
    "participant": (
        "What an admitted participant does on its own behalf: propose endeavors, "
        "fill seats, pledge, derive mandates, open activities, submit deliveries, "
        "record reviews."
    ),
    "runtime": (
        "The execution seam. Episode claim/start/complete, checkpoints, permit "
        "consumption, usage reports, placement admission — reached with a "
        "byomd-minted workload token bound to one <code>(episode, generation)</code>."
    ),
    "projection": (
        "Reads only. Society and participant projections, the charter history, the "
        "event ledger and its payloads, snapshots, and the recovery checkpoint. No "
        "operation on this surface mutates anything."
    ),
    "pre_auth": (
        "Version and capability negotiation, answerable before any authority is "
        "established. Not a socket of its own: these operations answer on every "
        "socket."
    ),
    "originating": (
        "The two recovery reads a caller aims at its own prior request — "
        "<code>idempotency_result</code> and <code>cursor_recover</code>. Not a "
        "socket of its own: they answer on the mutation-capable socket the "
        "original request used."
    ),
    "admin": (
        "Named in the closed surface enum and bound to <strong>no operation at "
        "all</strong>. There is no <code>admin.sock</code>, and deny-by-absence "
        "means an admin call is not unauthorized — it is unaddressable."
    ),
}

CLASS_MEANING = {
    "read": "Never mutates; carries no <code>MutationMeta</code>.",
    "create": (
        "A mutation whose closed meta has <strong>no</strong> "
        "<code>expected_revision</code> member at all, so supplying one fails the "
        "schema."
    ),
    "update": (
        "A current-head CAS whose closed meta <strong>requires</strong> "
        "<code>meta.expected_revision</code> (RT-01)."
    ),
}

DIGEST_CLASS_USE = {
    "structural_public": (
        "Knowingly non-sensitive, non-erasable protocol or schema bytes only. "
        "SHA-256 over type-tagged canonical bytes."
    ),
    "portable_public": (
        "Content whose owner explicitly accepted a durable, publicly "
        "dictionary-testable identifier. SHA-256 over exact bytes. This is the "
        "class the A8 cross-boundary rule demands of any digest one protocol asks "
        "the other for."
    ),
    "local_erasure_safe": (
        "Ordinary erasable local content and every authority subject. HMAC under a "
        "random <strong>per-object</strong> secret: destroying that secret destroys "
        "exactly that object's offline verification."
    ),
    "scope_erasure_safe": (
        "Shared-key index and chain constructions — the idempotency index, the "
        "privacy-access chain. HMAC under a <strong>per-scope</strong> key: "
        "destroying it erases verifiability for the whole scope, never one object."
    ),
    "disclosed_party": (
        "SHA-256 over bytes already disclosed to named recipients, always carrying "
        "the external-copy obligation."
    ),
    "ciphertext_public": (
        "SHA-256 over encrypted blob bytes. Sealed blobs only — never a commitment "
        "to low-entropy plaintext."
    ),
}

LIMIT_LABEL = {
    "REQUEST_MAX_BYTES": ("Request body", "bytes, inclusive — 256 KiB"),
    "RESPONSE_MAX_BYTES": ("Response body", "bytes, inclusive — 1 MiB"),
    "JSON_DEPTH_MAX": ("Container nesting depth", "levels, inclusive"),
    "JSON_NODES_MAX": ("JSON values per document", "nodes, inclusive"),
    "IDENTIFIER_MAX_BYTES": ("Identifier", "bytes of visible ASCII"),
    "MUTATION_LIST_ITEMS_MAX": ("List items per mutation", "items"),
    "EVENTS_PAGE_ITEMS_MAX": ("Events per page", "events"),
}

LIMIT_ORDER = [
    "REQUEST_MAX_BYTES", "RESPONSE_MAX_BYTES", "JSON_DEPTH_MAX",
    "JSON_NODES_MAX", "IDENTIFIER_MAX_BYTES", "MUTATION_LIST_ITEMS_MAX",
    "EVENTS_PAGE_ITEMS_MAX",
]

# The order surfaces are presented in: the five that have a socket, then the
# two pseudo-surfaces, then the one that binds nothing.
SURFACE_ORDER = ["governance", "candidate", "participant", "runtime",
                 "projection", "pre_auth", "originating", "admin"]

SOCKET_SURFACES = ["governance", "candidate", "participant", "runtime",
                   "projection"]


# ------------------------------------------------------------------ the page


def nav(here: str) -> str:
    pages = [
        ("concepts", "Concepts"),
        ("protocol", "Protocol"),
        ("encoding", "Encoding"),
        ("governed-work", "Governed work"),
        ("proofs", "Proofs"),
        ("security", "Security &amp; limits"),
        ("reference", "Reference"),
    ]
    out = ['  <p class="grp">Pages</p>']
    out.append('  <a href="../">Home</a>')
    for slug, label in pages:
        mark = ' class="here"' if slug == here else ""
        out.append(f'  <a href="../{slug}/"{mark}>{label}</a>')
    return "\n".join(out)


def build() -> str:
    rows = registry()
    enum = surface_enum()
    served = served_ops()
    all_ops = {r["operation"] for r in rows}
    by_surface: dict[str, list[dict]] = {}
    for row in rows:
        by_surface.setdefault(row["surface"], []).append(row)

    for surface in by_surface:
        if surface not in SURFACE_PURPOSE:
            die(f"the registry binds surface `{surface}` but this script has no "
                f"description for it — add one to SURFACE_PURPOSE")
    for surface in enum:
        if surface not in SURFACE_PURPOSE:
            die(f"the surface enum names `{surface}` but this script has no "
                f"description for it — add one to SURFACE_PURPOSE")

    classes = sorted({r["class"] for r in rows})
    for cls in classes:
        if cls not in CLASS_MEANING:
            die(f"registry class `{cls}` has no meaning in CLASS_MEANING")

    bundles = sorted({r.get("bundle", "B0.1") for r in rows})
    families = sorted({r["family"] for r in rows})
    machines = descriptors()
    modeled = modeled_descriptors()
    dclasses = digest_classes()
    lim = limits()
    kinds = problem_kinds()

    p: list[str] = []
    a = p.append

    a("<!doctype html>")
    a('<html lang="en">')
    a("<head>")
    a('<meta charset="utf-8">')
    a('<meta name="viewport" content="width=device-width, initial-scale=1">')
    a("<title>Reference — byom</title>")
    a('<meta name="description" content="The complete Byom Participation Protocol '
      'operation registry, per surface, generated from spec/registry.json — plus '
      'the transition machines, digest classes, problem kinds and wire limits.">')
    a('<link rel="canonical" href="https://byom.cc/reference/">')
    a('<link rel="icon" type="image/svg+xml" href="../favicon.svg">')
    a("<script>")
    a("/* Apply a saved theme before first paint; site.js wires the toggle. */")
    a("(function (root) {")
    a("  try {")
    a('    var saved = localStorage.getItem("byom-theme");')
    a('    if (saved === "light" || saved === "dark") root.setAttribute("data-theme", saved);')
    a("  } catch (e) {}")
    a("})(document.documentElement);")
    a("</script>")
    a('<link rel="stylesheet" href="../assets/site.css">')
    a("</head>")
    a("<body>")
    a('<a class="skip" href="#top">Skip to content</a>')
    a('<div class="shell">')
    a('<aside class="rail">')
    a('  <div class="rail-head">')
    a('    <a class="rail-mark" href="../">by<span class="g">o</span>m</a>')
    a('    <button class="themer" id="themer" type="button" aria-label="Switch colour theme">theme</button>')
    a("  </div>")
    a('  <p class="rail-sub">generated · do not edit</p>')
    a('  <nav id="nav" aria-label="Contents">')
    a('  <p class="grp">On this page</p>')
    a('  <a href="#registry">The registry</a>')
    a('  <a href="#surfaces">Operations by surface</a>')
    a('  <a href="#machines">Transition machines</a>')
    a('  <a href="#digests">Digest classes</a>')
    a('  <a href="#problems">Problem kinds</a>')
    a('  <a href="#limits">Wire limits</a>')
    a(nav("reference"))
    a("  </nav>")
    a("</aside>")
    a("<main id=\"top\">")

    a('<header class="hero">')
    a('  <p class="eyebrow">Generated reference</p>')
    a("  <h1>Every operation, and the surface it answers on.</h1>")
    a('  <p class="lede">This page is written by <code>docs-tools/gen_reference.py</code> '
      "out of the frozen registry, the schemas, the descriptors and the daemon's own "
      "op list. Nothing on it is typed by hand, and "
      "<code>./run-checks.sh</code> fails if it drifts from those sources.</p>")
    a('  <div class="pills">')
    a(f'    <span class="pill">{len(rows)} registry rows</span>')
    a(f'    <span class="pill">{len(all_ops)} operations</span>')
    a(f'    <span class="pill">{len(by_surface)} bound surfaces</span>')
    a(f'    <span class="pill">{len(bundles)} bundles</span>')
    a(f'    <span class="pill">{len(machines)} machines</span>')
    a("  </div>")
    a("</header>")

    # ---- the registry -------------------------------------------------
    a('<h2 id="registry" class="chapter-start">The registry</h2>')
    a("<p>The registry keys one row per <code>(operation, surface)</code> pair. It is "
      "the freeze source for a bundle and the dispatch truth for the daemon: a call "
      "whose <code>(operation, surface)</code> pair has no row is refused, not "
      "interpreted. Four operations carry two rows because they exist on both the "
      "participant and the governance surface.</p>")

    a('<div class="tw">')
    a('<table><thead><tr><th>Surface</th><th class="num">Rows</th><th>What it is</th></tr></thead><tbody>')
    for surface in SURFACE_ORDER:
        if surface not in SURFACE_PURPOSE:
            continue
        count = len(by_surface.get(surface, []))
        cls = ' class="limit"' if count == 0 else ""
        a(f'  <tr{cls}><td><code>{e(surface)}</code></td><td class="num">{count}</td>'
          f"<td>{SURFACE_PURPOSE[surface]}</td></tr>")
    a("</tbody></table>")
    a("</div>")

    a('<div class="tw">')
    a('<table class="narrow-first"><thead><tr><th>Class</th><th class="num">Rows</th>'
      "<th>What the meta looks like</th></tr></thead><tbody>")
    for cls in ("read", "create", "update"):
        if cls not in classes:
            continue
        count = sum(1 for r in rows if r["class"] == cls)
        a(f'  <tr><td><code>{e(cls)}</code></td><td class="num">{count}</td>'
          f"<td>{CLASS_MEANING[cls]}</td></tr>")
    a("</tbody></table>")
    a("</div>")

    a('<div class="tw">')
    a('<table class="narrow-first"><thead><tr><th>Bundle</th><th class="num">Rows</th>'
      "<th>Families</th></tr></thead><tbody>")
    for bundle in bundles:
        brows = [r for r in rows if r.get("bundle", "B0.1") == bundle]
        fams = sorted({r["family"] for r in brows})
        a(f'  <tr><td><code>{e(bundle)}</code></td><td class="num">{len(brows)}</td>'
          f'<td>{", ".join(f"<code>{e(f)}</code>" for f in fams)}</td></tr>')
    a("</tbody></table>")
    a("</div>")

    a(f"<p>Across {len(families)} families. The <em>served</em> column below is read "
      "out of the daemon's own <code>implemented()</code> predicate: "
      f"<strong>{len(served & all_ops)} of {len(all_ops)}</strong> operations are "
      "implemented, and the rest answer a typed <code>feature_unavailable</code> "
      "rather than being silently absent.</p>")

    # ---- per-surface tables -------------------------------------------
    a('<h2 id="surfaces" class="chapter-start">Operations by surface</h2>')
    a("<p>Request and result schema names are the frozen files under "
      f'<a href="{GH}/spec/schemas/ops">spec/schemas/ops/</a>; a <code>-v2</code> '
      "name is an immutable successor publication, never an edit of the original.</p>")

    for surface in SURFACE_ORDER:
        srows = by_surface.get(surface, [])
        a(f'<h3 id="surface-{e(surface)}"><code>{e(surface)}</code> '
          f"— {len(srows)} row{'' if len(srows) == 1 else 's'}</h3>")
        a(f"<p>{SURFACE_PURPOSE[surface]}</p>")
        if not srows:
            a('<div class="note limit"><span class="tag">Deny by absence</span>'
              f"<p>No operation is bound to <code>{e(surface)}</code>. It is a value "
              "in the closed surface enum with no registry row and no socket, so "
              "there is nothing to call and nothing to authorize.</p></div>")
            continue
        a('<div class="tw">')
        a('<table class="wide"><thead><tr><th>Operation</th><th>Binding</th>'
          "<th>Family</th><th>Class</th><th>Bundle</th><th>Request / result schema</th>"
          "<th>Served</th></tr></thead><tbody>")
        for row in sorted(srows, key=lambda r: r["operation"]):
            op = row["operation"]
            is_served = op in served
            a("  <tr>"
              f'<td><code>{e(op)}</code></td>'
              f'<td><code>{e(row["binding"])}</code></td>'
              f'<td>{e(row["family"])}</td>'
              f'<td>{e(row["class"])}</td>'
              f'<td>{e(row.get("bundle", "B0.1"))}</td>'
              f'<td><code>{e(row["request_schema"])}</code><br><code>{e(row["result_schema"])}</code></td>'
              f'<td>{"yes" if is_served else "<strong>no</strong>"}</td>'
              "</tr>")
        a("</tbody></table>")
        a("</div>")

    unserved = sorted(all_ops - served)
    if unserved:
        a('<div class="note limit"><span class="tag">Registry-bound, not implemented</span>')
        a(f"<p>{len(unserved)} of the {len(all_ops)} registered operations are not "
          "implemented by this daemon: "
          + ", ".join(f"<code>{e(op)}</code>" for op in unserved)
          + ". Each is bound in the registry and answers "
          "<code>feature_unavailable</code>; <code>feature_info</code> advertises "
          "exactly the implemented set, because a feature is advertised only when "
          "it is complete.</p></div>")

    # ---- machines -----------------------------------------------------
    total_states = sum(len(m.get("states", [])) for m in machines)
    total_transitions = sum(len(m.get("transitions", [])) for m in machines)
    kovee_owned = [m for m in machines if m.get("owner", "byom") != "byom"]

    a('<h2 id="machines" class="chapter-start">Transition machines</h2>')
    a(f"<p>{len(machines)} committed transition descriptors, {total_states} states and "
      f"{total_transitions} transitions in total. A descriptor is the closed "
      "state machine for one record kind: a transition that is not listed is not "
      "merely disallowed, it is invalid. "
      f"{len(kovee_owned)} of them are Kovee-owned executors, outside byom's "
      "one-descriptor-per-mutating-operation rule.</p>")
    a(f"<p>The <em>model</em> column names the TLA+ module that declares parity with "
      f"the descriptor. {len(tla_modules())} modules exist; a descriptor with no "
      "model is covered by the executable state walks and by the schema and registry "
      "gates, but not by TLC.</p>")
    a('<div class="tw">')
    a('<table class="wide"><thead><tr><th>Machine</th><th class="num">States</th>'
      '<th class="num">Transitions</th><th>Owner</th><th>Model</th></tr></thead><tbody>')
    for m in machines:
        model = modeled.get(m["_file"])
        cls = "" if model else ' class="limit"'
        a(f"  <tr{cls}><td><code>{e(m.get('machine', m['_file']))}</code></td>"
          f'<td class="num">{len(m.get("states", []))}</td>'
          f'<td class="num">{len(m.get("transitions", []))}</td>'
          f'<td>{e(m.get("owner", "byom"))}</td>'
          f'<td>{f"<code>{e(model)}.tla</code>" if model else "— none"}</td></tr>')
    a("</tbody></table>")
    a("</div>")

    # ---- digest classes -----------------------------------------------
    a('<h2 id="digests" class="chapter-start">Digest classes</h2>')
    a("<p>Every digest field on the wire is a typed <code>DigestRef</code> — "
      "<code>{class, algorithm, key_ref?, value_hex}</code>, a closed member set — "
      "never an unlabelled hash. A well-constructed digest of the wrong class is "
      "<code>digest_class_mismatch</code> even when the 32-byte value spaces "
      "coincide.</p>")
    a('<div class="tw">')
    a('<table class="wide"><thead><tr><th>Class</th><th>Algorithm</th><th><code>key_ref</code></th>'
      "<th>What it is for</th></tr></thead><tbody>")
    for name, algorithm in dclasses.items():
        if name not in DIGEST_CLASS_USE:
            die(f"digest class `{name}` has no description in DIGEST_CLASS_USE")
        keyed = "required" if algorithm == "hmac-sha-256" else "forbidden"
        a(f"  <tr><td><code>{e(name)}</code></td><td><code>{e(algorithm)}</code></td>"
          f"<td>{keyed}</td><td>{DIGEST_CLASS_USE[name]}</td></tr>")
    a("</tbody></table>")
    a("</div>")

    # ---- problem kinds ------------------------------------------------
    a('<h2 id="problems" class="chapter-start">Problem kinds</h2>')
    a(f"<p>The failure envelope wraps an RFC 9457 problem object whose "
      f"<code>kind</code> is this closed {len(kinds)}-value enum. "
      "<code>type</code> is exactly "
      "<code>https://byom.dev/problems/</code> + <code>kind</code>; an unknown kind "
      "fails closed. <code>title</code> is free prose and carries no authority.</p>")
    a("<p>" + ", ".join(f"<code>{e(k)}</code>" for k in kinds) + ".</p>")

    # ---- limits -------------------------------------------------------
    a('<h2 id="limits" class="chapter-start">Wire limits</h2>')
    a("<p>The conformance-tested initial limits, advertised by "
      "<code>protocol_info</code> and enforced before schema validation.</p>")
    a('<div class="tw">')
    a('<table class="narrow-first"><thead><tr><th>Limit</th><th class="num">Value</th>'
      "<th>Unit</th></tr></thead><tbody>")
    for key in LIMIT_ORDER:
        label, unit = LIMIT_LABEL[key]
        a(f'  <tr><td>{e(label)}</td><td class="num">{lim[key]:,}</td>'
          f"<td>{e(unit)}</td></tr>")
    a("</tbody></table>")
    a("</div>")

    a('<nav class="pagenav" aria-label="Previous and next page">')
    a('  <a class="prev" href="../security/"><span class="k">Previous</span>Security &amp; limits</a>')
    a('  <a class="next" href="../"><span class="k">Next</span>Home</a>')
    a("</nav>")

    a("<footer>")
    a("  <p>Generated by <code>docs-tools/gen_reference.py</code> from "
      f'<a href="{GH}/spec/registry.json">spec/registry.json</a>, '
      f'<a href="{GH}/spec/schemas">spec/schemas/</a>, '
      f'<a href="{GH}/spec/descriptors">spec/descriptors/</a>, '
      f'<a href="{GH}/proof/specs">proof/specs/</a> and the daemon sources. '
      "Editing this file by hand fails <code>./run-checks.sh</code>.</p>")
    a("</footer>")

    a("</main>")
    a("</div>")
    a('<script src="../assets/site.js"></script>')
    a("</body>")
    a("</html>")
    return "\n".join(p) + "\n"


def main() -> int:
    page = build()
    check = "--check" in sys.argv[1:]
    if check:
        if not OUT.exists():
            print("gen_reference: docs/reference/index.html does not exist — run "
                  "`python3 docs-tools/gen_reference.py`", file=sys.stderr)
            return 1
        if read(OUT) != page:
            print("gen_reference: docs/reference/index.html is STALE — the sources "
                  "moved and the page did not.\n"
                  "        fix: python3 docs-tools/gen_reference.py", file=sys.stderr)
            return 1
        print(f"gen_reference: docs/reference/index.html is current "
              f"({len(registry())} registry rows)")
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(page, encoding="utf-8")
    print(f"gen_reference: wrote {OUT.relative_to(ROOT)} ({len(page)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
