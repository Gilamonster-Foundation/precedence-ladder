# Changelog

All notable changes to `precedence-ladder` are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project
uses [Semantic Versioning](https://semver.org/) (Rust SemVer on crates.io, the
equivalent PEP 440 spelling on PyPI once the Python face lands).

| | Registry | Package name | Import / crate name |
| --- | --- | --- | --- |
| Rust core | crates.io | `precedence-ladder` | `precedence_ladder` (crate) |
| Python (slice C3) | PyPI | `precedence-ladder` | `import precedence_ladder` |

## [Unreleased] — `0.1.0`

**Nothing is published yet.** `0.1.0` is assembled across slices C1–C3; the
crates.io and PyPI publishes are gated on a later slice and an operator
decision.

### Added — slice C1: the crate and `resolve`

- **`Hatch`** — the operator's escape hatch: a reserved trigger set and an
  action label. **Non-empty by construction**: `Hatch::new(action, first, rest)`
  takes a first element plus a rest, so an empty reserved set — which would make
  the escape-hatch invariant vacuous — has no spelling. No `Default`, no
  `new(Vec)`, no `Result`, and deliberately **no `Deserialize`** (a wire form
  would otherwise mint an empty hatch without calling the constructor).
- **`Rung`** — one row: a claimant name, the triggers it owns, an action label.
- **`Ladder`** — the ordered table. `Ladder::new` is **infallible**: a rung that
  claims a reserved trigger is inert by resolution order, so there is nothing
  to reject and no validation step a consumer can forget.
- **`ClaimSet`** / **`Situation`** / **`Verdict`** — which claimants are live,
  what the moment is, and the answer. All plain values: no trait objects, no
  closures, no builder-with-callbacks, because those die at the PyO3 and
  wasm-bindgen boundaries.
- **`Ladder::resolve`** — the predicate. Hatch-while-working, then the rungs in
  table order restricted to live claimants, then fallthrough-while-working, then
  `Unbound`. **The order of those branches is the contract**: the reserved check
  precedes the rung scan, which is what makes the escape-hatch invariant
  unrepresentable rather than validated.
- **`Ladder::describe`** — what a trigger *would* do, from the same table by the
  same traversal, so an affordance cannot advertise something `resolve` will not
  do. Returns a label borrowed from the ladder, never a crate constant, so the
  vocabulary belongs to the consumer and is localizable.
- **`Ladder::collisions`** — the authoring lint. Reports rows that can never
  fire: a rung claiming a reserved trigger, a fallthrough trigger the hatch also
  reserves, and a repeated `(claimant, trigger)` pair. Two *different* claimants
  sharing a trigger is not a collision — that is what an ordered ladder is for.
- **`Ladder::claimants`** — drives a consumer's registration conformance test.
- **`Ladder::from_toml`** (feature `table`, **on by default**) — the ladder as
  data, with `deny_unknown_fields` and a `schema = 1` check. An empty
  `[hatch] reserved` is a parse error, which is the one place non-emptiness is
  checked rather than unrepresentable: it is a parse boundary, so the check
  happens exactly once, at the only door untyped data comes through.
- **`ContentAddressable for Ladder`** (feature `cid`, **off by default**) — a
  table's content identity as a CIDv1 / dag-cbor / BLAKE3 `ContentId`, minted by
  the `content-addressable` crate. Never a hand-rolled digest. Off by default
  because that stack would bloat a wasm build of a pure predicate. **These bytes
  are not frozen**; the golden vectors that pin them land in slice C2.
- **`NO_FALLTHROUGH`** — the empty fallthrough set, spelled so inference works.

### Guarded

- **`tests/truth_table.rs`** — the exhaustive grid: 2^5 claim sets × 3 triggers
  × 2 work states = **192 cases**, asserted against a hand-derived
  *distribution* (33 escapes, 62 claims, 97 unbound; winners 32/16/8/4/2) rather
  than against a second implementation of `resolve`. `describe` is checked
  against `resolve` on every one of the 192. Anti-vacuous twins: reversing the
  rungs must invert the winner distribution, moving the reservation must move
  the escapes, and a rogue rung claiming the reserved trigger at the top of the
  ladder must still lose it across the whole claim space *while remaining live
  and winning on its other trigger*.
- **`tests/guard.rs`** — the dependency direction, armed. With
  `--no-default-features` the resolved closure is **empty**; every declared
  runtime dependency is optional; no feature reaches a terminal, runtime, or CLI
  crate; and no line of `src/` touches `std::{fs,net,process,env}`. Each half
  carries an anti-vacuous twin pointed at a target known to violate it.
- **`scripts/check-leaf-deps.sh`** — the closure stays registry-only (no
  path/git/workspace edges) at every feature setting, so any repo can depend on
  this crate without cycles and the `cid` byte contract cannot be swapped
  without a crates.io version bump.
- **`scripts/verify_release.py`** — the single implementation of the SemVer ↔
  PEP 440 mapping, with a `--self-test` that CI runs on every PR.
- **`justfile`**, **`.githooks/pre-push`**, **`.github/workflows/ci.yml`** — the
  hook mirrors the pipeline and each names the other in a parity comment. The
  whole gate is seconds; do not let it grow into a hook people route around.

### Not in C1

The Lean layer and golden vectors (C2); the PyO3/PyPI face, the release
workflow, and the `0.1.0` publish (C3); the wasm/npm package (C4).
