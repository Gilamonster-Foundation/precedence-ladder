//! The exhaustive truth table — the primary evidence for this crate.
//!
//! `resolve` is a pure function of three things: the ladder, the trigger, and
//! the situation. For the shipped five-claimant table that makes the whole
//! input space small enough to enumerate: **2^5 claim sets × 3 triggers × 2
//! work states = 192 cases**, every one of them checked.
//!
//! The grid is not asserted case-by-case against a hand-written expectation —
//! that would be a second implementation of `resolve`, and it would agree with
//! the first by construction. It is asserted against a **distribution**: exact
//! counts of each verdict kind and of each winning claimant, derived by hand
//! from the ladder's shape. Those numbers are what a mutation moves.
//!
//! Every claim here carries an ANTI-VACUOUS TWIN, stated in the test that
//! carries it: what would make it fail if the thing it guards were absent.

use precedence_ladder::{ClaimSet, Hatch, Ladder, Rung, Situation, Verdict};
use std::collections::BTreeMap;

/// The five claimants of the shipped table, in precedence order. The ORDER of
/// this array is load-bearing for every expected count below.
const CLAIMANTS: [&str; 5] = ["palette", "vi-confirm", "vi-ex", "vi-insert", "vi-pending"];

/// `esc` (fallthrough), `ctrl-c` (reserved), and one trigger the table has
/// never heard of.
const TRIGGERS: [&str; 3] = ["esc", "ctrl-c", "ctrl-x"];

/// The shipped table: Ctrl-C reserved, Esc falling through to the hatch once
/// every claimant has declined.
fn newt_ladder() -> Ladder {
    Ladder::new(
        Hatch::new("interrupt", "ctrl-c", []),
        vec![
            Rung::new("palette", ["esc"], "close palette"),
            Rung::new("vi-confirm", ["esc"], "cancel [y/N]"),
            Rung::new("vi-ex", ["esc"], "cancel :"),
            Rung::new("vi-insert", ["esc"], "NORMAL"),
            Rung::new("vi-pending", ["esc"], "cancel operator"),
        ],
        ["esc"],
    )
}

/// The `bits`-th subset of [`CLAIMANTS`].
fn claim_set(bits: u32) -> ClaimSet {
    CLAIMANTS
        .iter()
        .enumerate()
        .filter(|(i, _)| bits & (1 << i) != 0)
        .map(|(_, name)| *name)
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Tally {
    cases: usize,
    claimed: usize,
    escape: usize,
    unbound: usize,
    /// claimant name -> how many cases it won.
    winners: BTreeMap<String, usize>,
}

/// Walk the entire input space of `ladder` over `TRIGGERS`, checking the
/// per-case invariants and tallying the distribution.
fn walk(ladder: &Ladder) -> Tally {
    let mut t = Tally::default();
    for bits in 0..(1u32 << CLAIMANTS.len()) {
        let claiming = claim_set(bits);
        for trigger in TRIGGERS {
            for work_running in [true, false] {
                let s = Situation {
                    claiming: &claiming,
                    work_running,
                };
                let verdict = ladder.resolve(trigger, &s);
                t.cases += 1;

                // THE ESCAPE-HATCH INVARIANT, checked at every point of the
                // grid rather than at one hand-picked example. No value of
                // `rungs` — including the rogue tables below — may affect it.
                if work_running && ladder.hatch().reserved().any(|r| r == trigger) {
                    assert!(
                        matches!(verdict, Verdict::Escape { .. }),
                        "reserved trigger {trigger:?} did not escape while work \
                         was running, claiming {:?}: {verdict:?}",
                        claiming.names().collect::<Vec<_>>()
                    );
                }

                // `describe` reads the SAME table by the SAME traversal, so an
                // affordance can never advertise something `resolve` will not
                // do. Checked on all 192 cases, not asserted in prose.
                assert_eq!(
                    ladder.describe(trigger, &s),
                    verdict.action(),
                    "describe disagreed with resolve on {trigger:?} \
                     (work_running={work_running}, claiming {:?})",
                    claiming.names().collect::<Vec<_>>()
                );

                match verdict {
                    Verdict::Claimed { claimant, .. } => {
                        t.claimed += 1;
                        *t.winners.entry(claimant.to_string()).or_default() += 1;
                    }
                    Verdict::Escape { .. } => t.escape += 1,
                    Verdict::Unbound => t.unbound += 1,
                }
            }
        }
    }
    t
}

/// **The whole input space, with the distribution derived by hand.**
///
/// | trigger | work | outcome | count |
/// |---|---|---|---|
/// | `ctrl-c` | running | `Escape` (reserved, hatch-first) | 32 |
/// | `ctrl-c` | idle | `Unbound` (no rung binds it; no work to escape) | 32 |
/// | `esc` | running | `Claimed` by the first live rung | 31 |
/// | `esc` | running | `Escape` (all five declined — rung 7) | 1 |
/// | `esc` | idle | `Claimed` by the first live rung | 31 |
/// | `esc` | idle | `Unbound` (all declined, and no work to escape) | 1 |
/// | `ctrl-x` | either | `Unbound` (unbound trigger) | 64 |
///
/// Winners follow from first-match: a claimant wins exactly the subsets that
/// contain it and no earlier claimant — 16, 8, 4, 2, 1 per work state, so
/// doubled across the two.
#[test]
fn the_whole_input_space() {
    let t = walk(&newt_ladder());

    assert_eq!(
        t.cases,
        (1usize << CLAIMANTS.len()) * TRIGGERS.len() * 2,
        "the grid did not cover the whole input space"
    );
    assert_eq!(t.cases, 192, "2^5 claim sets x 3 triggers x 2 work states");
    assert_eq!(
        t.escape, 33,
        "32 reserved-while-running + 1 esc fallthrough"
    );
    assert_eq!(t.claimed, 62, "31 live-claimant subsets x 2 work states");
    assert_eq!(
        t.unbound, 97,
        "64 unbound trigger + 32 idle ctrl-c + 1 idle esc"
    );
    assert_eq!(
        t.claimed + t.escape + t.unbound,
        t.cases,
        "cases unaccounted"
    );

    // ANTI-VACUOUS TWIN (a): no verdict kind may be absent. A grid that only
    // ever produced `Unbound` would satisfy a "sums to 192" check and prove
    // nothing about precedence.
    for (name, n) in [
        ("claimed", t.claimed),
        ("escape", t.escape),
        ("unbound", t.unbound),
    ] {
        assert!(n > 0, "the grid never produced a single {name} verdict");
    }

    assert_eq!(
        t.winners,
        BTreeMap::from([
            ("palette".to_string(), 32),
            ("vi-confirm".to_string(), 16),
            ("vi-ex".to_string(), 8),
            ("vi-insert".to_string(), 4),
            ("vi-pending".to_string(), 2),
        ]),
        "first-match-wins distribution changed"
    );

    // ANTI-VACUOUS TWIN (b): every claimant in the table must win somewhere.
    // A rung that can never fire would make a conformance test pass on a
    // constant — the reason the shipped table has no `modal` rung.
    for c in CLAIMANTS {
        assert!(t.winners.contains_key(c), "rung {c:?} never fired");
    }
}

/// **ANTI-VACUOUS TWIN for the distribution itself.** Reverse the rungs and
/// the winner counts must invert. Without this the harness could be measuring
/// something insensitive to the table, and `the_whole_input_space` would be a
/// snapshot of an accident rather than of first-match-wins.
///
/// This is also the Rust statement of `order_is_load_bearing`: a ladder whose
/// verdict is invariant under a permutation of its rungs is a *set*, and the
/// crate would not need to exist.
#[test]
fn reversing_the_rungs_inverts_the_distribution() {
    let mut rungs = newt_ladder().rungs().to_vec();
    rungs.reverse();
    let reversed = Ladder::new(Hatch::new("interrupt", "ctrl-c", []), rungs, ["esc"]);

    let t = walk(&reversed);
    // Totals per verdict KIND are order-invariant — only the winner changes.
    assert_eq!((t.cases, t.claimed, t.escape, t.unbound), (192, 62, 33, 97));
    assert_eq!(
        t.winners,
        BTreeMap::from([
            ("vi-pending".to_string(), 32),
            ("vi-insert".to_string(), 16),
            ("vi-ex".to_string(), 8),
            ("vi-confirm".to_string(), 4),
            ("palette".to_string(), 2),
        ]),
        "reversing the table did not invert the winners — the harness is not \
         measuring rung order"
    );
}

/// **ANTI-VACUOUS TWIN for the hatch check inside [`walk`].** In the shipped
/// table only `ctrl-c` is reserved, so the invariant assertion fires on a third
/// of the grid. Reserve `esc` instead and it must fire on a *different* third —
/// and the distribution must move, proving the assertion is keyed to the ladder
/// under test rather than to a hard-coded trigger name.
#[test]
fn moving_the_reservation_moves_the_escapes() {
    let moved = Ladder::new(
        Hatch::new("interrupt", "esc", []),
        newt_ladder().rungs().to_vec(),
        ["esc"],
    );
    let t = walk(&moved);

    // esc + running -> Escape for all 32 subsets (hatch-first beats every
    // rung); esc + idle -> Claimed for 31, Unbound for 1; ctrl-c and ctrl-x
    // are now bound to nothing at all.
    assert_eq!(t.escape, 32, "every running esc must reach the hatch");
    assert_eq!(t.claimed, 31, "only the idle half can still be claimed");
    assert_eq!(t.unbound, 129);
    assert_eq!(t.cases, 192);
    assert_ne!(
        t.winners,
        walk(&newt_ladder()).winners,
        "moving the reservation left the distribution unchanged"
    );
}

/// **The escape hatch survives a hostile table.**
///
/// A rogue rung sits at the TOP of the ladder and claims the reserved trigger.
/// Across the entire grid it must never win `ctrl-c` while work is running —
/// not because the constructor rejected it (it does not; `Ladder::new` is
/// infallible) but because the reserved branch precedes the rung scan.
#[test]
fn a_rogue_rung_cannot_swallow_the_hatch() {
    let mut rungs = vec![Rung::new("rogue", ["ctrl-c", "zz"], "swallow")];
    rungs.extend(newt_ladder().rungs().to_vec());
    let rogue = Ladder::new(Hatch::new("interrupt", "ctrl-c", []), rungs, ["esc"]);

    let mut checked = 0;
    for bits in 0..(1u32 << CLAIMANTS.len()) {
        let mut claiming = claim_set(bits);
        claiming.claiming("rogue");
        let running = Situation {
            claiming: &claiming,
            work_running: true,
        };
        assert_eq!(
            rogue.resolve("ctrl-c", &running),
            Verdict::Escape {
                action: "interrupt"
            },
            "the rogue rung swallowed the hatch with claims {:?}",
            claiming.names().collect::<Vec<_>>()
        );

        // ANTI-VACUOUS TWIN: the rogue rung must be live and otherwise
        // winning. If it never fired, the assertion above would hold for a
        // reason that has nothing to do with resolution order — and would keep
        // holding after the reserved check was deleted.
        assert_eq!(
            rogue.resolve("zz", &running),
            Verdict::Claimed {
                claimant: "rogue",
                action: "swallow"
            },
            "the rogue rung is not live — the test above proves nothing"
        );
        checked += 1;
    }
    assert_eq!(checked, 32, "the rogue sweep did not cover the claim space");

    // And the rogue row is reported, so an author learns at load time.
    assert_eq!(rogue.collisions().len(), 1, "{:?}", rogue.collisions());
}

/// The table read from TOML is the table built in code.
///
/// This is what makes "knowledge as data" more than a slogan: everything the
/// grid above proves about the constructed ladder holds for the shipped `.toml`
/// too, because they are the same value.
///
/// It is also the join between this file and the formal layer.
/// `spec/vectors/newt_ladder.toml` is compiled into `examples/gen_vectors.rs`,
/// which emits `spec/vectors/ladder.json` and the Lean `decide` block from it.
/// Reading the same file here — rather than keeping a second copy inline — is
/// what makes "the grid the tests walk" and "the grid the vectors record" the
/// same claim rather than two that agree today.
#[cfg(feature = "table")]
#[test]
fn the_shipped_toml_is_the_ladder_the_grid_walked() {
    const SHIPPED: &str = include_str!("../spec/vectors/newt_ladder.toml");
    let parsed = Ladder::from_toml(SHIPPED).expect("the shipped table parses");
    assert_eq!(parsed, newt_ladder());
    assert_eq!(parsed.collisions(), vec![], "the shipped table is clean");
    // ANTI-VACUOUS TWIN: equality on an empty ladder would also hold, so
    // assert the parse actually produced the five rows.
    assert_eq!(parsed.claimants().collect::<Vec<_>>(), CLAIMANTS.to_vec());
    assert_eq!(walk(&parsed), walk(&newt_ladder()));
}
