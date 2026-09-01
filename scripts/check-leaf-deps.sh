#!/usr/bin/env bash
# check-leaf-deps.sh — the LEAF invariant guard.
#
# `precedence-ladder` is meant to be pulled into other people's harnesses —
# newt, wyvern, gilamonster, and any foreign TUI that wants the policy without
# newt's release train. That only holds if it stays a true LEAF: its resolved
# dependency closure must contain ZERO path, git, or other non-crates.io-registry
# edges, AT EVERY FEATURE SETTING. The moment it gains a path/git dep, a
# downstream repo either fails to build from crates.io or pulls in an
# un-publishable cycle — and a path/git dep can silently swap the
# canonicalization under the `cid` feature's content ids without a crates.io
# version bump, which is a byte-contract concern, not just packaging.
#
# This is the COMPLEMENT of tests/guard.rs, not a duplicate of it:
#
#   tests/guard.rs  asserts the DEFAULT closure is EMPTY, and that no forbidden
#                   crate appears at any feature setting.
#   this script     asserts every dependency that IS there comes from crates.io.
#
# It reads only `cargo metadata` (no build, no network beyond index resolution),
# so it is fast and mirrors cleanly into the push hook.
#
# PIPELINE PARITY: this is the `leaf-deps` job in .github/workflows/ci.yml and
# the `just leaf` recipe. It also runs in .githooks/pre-push (true parity —
# `cargo metadata` is cheap). When editing this file, keep those three call
# sites in sync.
set -euo pipefail

CORE_PKG="precedence-ladder"

# `--all-features` so the OPTIONAL `cid` and `table` closures are checked too:
# a path dependency hidden behind an off-by-default feature is still shipped to
# whoever turns that feature on.
metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --format-version 1 --all-features >"$metadata_file"

CORE_PKG="$CORE_PKG" METADATA_FILE="$metadata_file" python3 - <<'PY'
import json
import os
import sys

core_name = os.environ["CORE_PKG"]
with open(os.environ["METADATA_FILE"], encoding="utf-8") as fh:
    md = json.load(fh)

packages = {p["id"]: p for p in md["packages"]}
resolve = md.get("resolve")
if resolve is None:
    sys.exit("leaf guard: `cargo metadata` returned no resolve graph (cannot "
             "compute the dependency closure).")
nodes = {n["id"]: n for n in resolve["nodes"]}

core_ids = [pid for pid, p in packages.items() if p["name"] == core_name]
if len(core_ids) != 1:
    sys.exit(f"leaf guard: expected exactly one '{core_name}' package, "
             f"found {len(core_ids)}: {core_ids}")
core_id = core_ids[0]

# Walk the FORWARD dependency closure rooted at the core package. A future
# `precedence-ladder-py -> precedence-ladder { path = ".." }` edge (slice C3)
# points AT the core, so it is a reverse dep and is never reached here — the
# guard tolerates it by construction rather than by a special case.
seen = set()
stack = [core_id]
while stack:
    cur = stack.pop()
    if cur in seen:
        continue
    seen.add(cur)
    for dep in nodes[cur]["deps"]:
        stack.append(dep["pkg"])

REGISTRY_PREFIX = "registry+"
offenders = []
for pid in seen:
    if pid == core_id:
        # The root crate itself is a local path (it IS this repo); expected,
        # and not a dependency edge.
        continue
    pkg = packages[pid]
    source = pkg.get("source")
    if source is None or not source.startswith(REGISTRY_PREFIX):
        offenders.append((pkg["name"], pkg.get("version", "?"),
                          source if source is not None else "path/local"))

if offenders:
    print(f"LEAF GUARD FAILED: '{core_name}' has non-registry dependencies in "
          f"its closure:", file=sys.stderr)
    for name, version, source in sorted(offenders):
        print(f"  - {name} {version}  (source: {source})", file=sys.stderr)
    print("\nThis crate MUST be a true leaf: every dependency in its closure "
          "must come from the crates.io registry (no path/git/workspace edges), "
          "so any repo can depend on it without cycles and the `cid` byte "
          "contract can't be swapped without a crates.io version bump.",
          file=sys.stderr)
    sys.exit(1)

count = len(seen) - 1  # exclude the core root itself
print(f"leaf guard OK: '{core_name}' closure is registry-only "
      f"({count} dependencies at --all-features, all from crates.io).")
PY
