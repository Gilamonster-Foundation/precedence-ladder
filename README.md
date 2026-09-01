# precedence-ladder

> **Who owns this trigger, right now — and can the operator always get out?**

A pure, dependency-free predicate for resolving keystroke precedence in an
interactive program. You declare a **ladder** as data; it answers, per event,
which registered claimant owns a trigger, or whether the trigger is the
operator's escape hatch, or whether nothing is bound.

It is deliberately small and honest: it is the instrument, not the sky. It
decodes no keys, owns no terminal, renders nothing, and dispatches nothing. It
returns a verdict; the host performs the effect.

```rust
use precedence_ladder::{ClaimSet, Hatch, Ladder, Rung, Situation, Verdict};

let ladder = Ladder::new(
    Hatch::new("interrupt", "ctrl-c", []),
    vec![Rung::new("vi-insert", ["esc"], "NORMAL")],
    ["esc"],
);

let mut claiming = ClaimSet::default();
claiming.claiming("vi-insert");
let working = Situation { claiming: &claiming, work_running: true };

// vi INSERT owns Esc...
assert_eq!(
    ladder.resolve("esc", &working),
    Verdict::Claimed { claimant: "vi-insert", action: "NORMAL" }
);
// ...but nothing owns the escape hatch.
assert_eq!(ladder.resolve("ctrl-c", &working), Verdict::Escape { action: "interrupt" });
```

## The escape-hatch invariant

> For every ladder `L`, every situation `s` with `s.work_running = true`, and
> every trigger `t` in `L`'s reserved set: `resolve(L, t, s) = Escape`.

**It is unrepresentable, not validated.** The reserved check *precedes* the
rung scan in [`Ladder::resolve`], so no value of `rungs` can affect it. There
is no fallible constructor to make, and nothing a consumer can forget to call
— a rung that claims a reserved trigger is simply inert, and
[`Ladder::collisions`] reports it as an authoring lint rather than an error.

Non-emptiness of the reserved set — the thing that keeps the statement from
being vacuous — is by construction too: [`Hatch::new`] takes a *first* element
plus a *rest*, so an empty reserved set cannot be spelled.

**Scope, stated so nobody overclaims it:** this is a property of *one ladder*.
It is not the claim "the operator can always get out of the application". That
harness-level claim needs every input context to have a ladder, and is
discharged in the consumer by a registration ratchet, not here.

## Knowledge as data

The ladder is a TOML table, not a hardcoded `match`:

```toml
schema = 1
fallthrough = ["esc"]

[hatch]
action = "interrupt"
reserved = ["ctrl-c"]

[[rung]]
claimant = "palette"
triggers = ["esc"]
action = "close palette"

[[rung]]
claimant = "vi-insert"
triggers = ["esc"]
action = "NORMAL"
```

`Ladder::from_toml` parses it with `deny_unknown_fields` and a `schema = 1`
check. Adding a claimant is config, not code.

## What it is not

| Excluded | Why |
|---|---|
| crossterm / ratatui / any terminal type | A `KeyEvent` in the signature forks the crate on day one. `Trigger` is an opaque string. |
| Key decoding, chord normalization, modifier parsing | That is "what a chord *means*". This answers "who owns it *now*". Different question, different library. |
| Escalation tiers, press counters, grace windows | Per-press state belongs to the consumer, which already has it. A copy here is a second reset path. |
| Interrupt delivery, tty ownership, modal preemption | The crate returns a verdict; the host performs the effect. |
| Trait-object claimants, builders with closures | They put knowledge back into logic, and they die at the PyO3 and wasm boundaries. |

**The whole API is plain values** — strings, sets of strings, booleans, and an
enum of strings. That is what lets it cross into Python and JavaScript
unchanged, and it is the reason there are no traits to implement.

### Stringly-typed claimant names, on the record

`Verdict::Claimed` carries a *name*, and a typo in the table yields a silent
no-op rather than a compile error. In the intended use the name does not
dispatch — the claimant re-derives its own handling, and the name feeds
`describe()` and a registration conformance test in the consumer. **A consumer
that does dispatch on the name must pair it with an exhaustive match**, and
should assert that every name in [`Ladder::claimants`] is answered by its own
claim accessors.

## Features

| Feature | Default | Pulls in | For |
|---|---|---|---|
| `table` | **on** | `serde`, `toml` | `Ladder::from_toml` — knowledge as data |
| `serde` | via `table` | `serde` | `Serialize`/`Deserialize` on the public types |
| `cid` | off | `content-addressable` | `ContentAddressable for Ladder` — a table's content id |

**The core predicate depends on nothing.** With `--no-default-features` the
resolved dependency closure of this crate is *empty*, and
`tests/guard.rs` asserts exactly that rather than describing it in a comment.

`cid` is off by default on purpose: `content-addressable` pulls BLAKE3 and the
multiformats stack, which would make a wasm artifact an order of magnitude
larger than a pure predicate has any business being.

## Status

**Unreleased, `0.1.0` in progress.** This is slice C1 — the crate skeleton and
`resolve`. The Lean layer (C2), the PyO3/PyPI face and the release workflow
(C3), and the wasm/npm package (C4) land in later slices. Nothing is published
to crates.io or PyPI yet.

## Build

```bash
just check          # fmt + clippy + test + doc + leaf  (the local gate)
just install-hooks  # wire .githooks/ as core.hooksPath
just msrv           # build + test on the pinned MSRV
```

## License

Apache-2.0.
