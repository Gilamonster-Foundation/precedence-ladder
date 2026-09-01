//! Golden-vector codegen — the only bridge between the Rust and the Lean.
//!
//! Run it with `just gen-vectors`. It reads the demo table
//! (`spec/vectors/newt_ladder.toml`, compiled in, so the generator itself reads
//! no file it was not built with), walks the whole input space of each grid
//! through the real `Ladder::resolve`, and writes two artifacts:
//!
//! * `spec/vectors/ladder.json` — the vectors as data, for consumers that can
//!   parse JSON (slice C3's stdlib-only Python conformance script).
//! * `formal/Precedence/Vectors.lean` — the same vectors as a Lean `decide`
//!   block, because a Mathlib-free Lean library has no JSON reader.
//!
//! **Why codegen rather than a hand transcription.** "One artifact, three
//! consumers" is a claim about drift. Without a generator plus a diff check it
//! is one artifact and two copies somebody promised to keep in step — and the
//! Lean copy is exactly the one nobody would notice going stale, because the
//! proofs would still pass about a table that no longer matches the code.
//! `just check-vectors` regenerates into a temp dir and diffs; CI and the
//! pre-push hook both run it.
//!
//! **The vectors ARE the evidence that the Lean model tracks the Rust.** Every
//! theorem in `Precedence.Basic` is a statement about the Lean transliteration
//! and, on its own, proves nothing about `src/lib.rs`. What ties the two
//! together is this file: verdicts produced by the real `resolve` and
//! re-derived by `decide` against the model. A transliteration error shows up
//! as a red `lake build`.
//!
//! **Why there are two grids, and not one.** The first draft emitted only the
//! demo table. Demoting the hatch branch below the rung scan in `resolve` — the
//! single most important thing the vectors are supposed to witness — changed
//! *nothing* in the output, because no rung of the demo table claims a reserved
//! trigger, so the grid cannot tell hatch-first from hatch-last. The `rogue`
//! grid is a hostile table whose TOP rung claims the reserved trigger; it is
//! what makes resolution order observable in the vectors, and it is why
//! `formal/README.md` records that mutation, and the others these gates were
//! verified against, under "Mutations these gates were checked on".

use content_addressable::{canonical, ContentAddressable, ContentError};
use precedence_ladder::{ClaimSet, Ladder, Rung, Situation, Verdict};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The demo table, compiled in. `tests/truth_table.rs` reads the same file, so
/// the grid the tests walk and the grid the vectors record cannot diverge.
const LADDER_TOML: &str = include_str!("../spec/vectors/newt_ladder.toml");

/// A trigger no table here binds, so every case using it must be `Unbound`.
/// Without it the grids would only ever exercise bound triggers and could not
/// distinguish "nothing matched" from "the scan never ran".
const UNBOUND_PROBE: &str = "ctrl-x";

/// The vector-file schema. Bump it when the JSON shape changes.
const SCHEMA: u32 = 1;

/// A ladder with more claimants than this makes the powerset walk unreasonable.
/// The generator refuses rather than emitting a file nobody can check.
const MAX_CLAIMANTS: usize = 8;

/// One golden vector: an input, and the verdict the real `resolve` produced.
#[derive(Serialize)]
struct Case {
    trigger: String,
    claiming: Vec<String>,
    work_running: bool,
    verdict: VerdictRec,
}

/// A [`Verdict`] as data. Three explicit fields rather than an externally
/// tagged enum: this is read by a stdlib-only Python script, and a flat record
/// needs no discriminated-union decoding on the far side.
#[derive(Serialize)]
struct VerdictRec {
    kind: &'static str,
    claimant: Option<String>,
    action: Option<String>,
}

impl VerdictRec {
    fn of(v: Verdict<'_>) -> VerdictRec {
        match v {
            Verdict::Claimed { claimant, action } => VerdictRec {
                kind: "claimed",
                claimant: Some(claimant.to_string()),
                action: Some(action.to_string()),
            },
            Verdict::Escape { action } => VerdictRec {
                kind: "escape",
                claimant: None,
                action: Some(action.to_string()),
            },
            Verdict::Unbound => VerdictRec {
                kind: "unbound",
                claimant: None,
                action: None,
            },
        }
    }
}

/// One ladder and the exhaustive walk of its input space.
#[derive(Serialize)]
struct Grid {
    /// Identifier stem: `demo` becomes `demoLadder` / `demoCases` in Lean.
    name: &'static str,
    /// What this grid is for, rendered into the generated Lean doc comment.
    note: &'static str,
    ladder: Ladder,
    ladder_content_id: String,
    claimants: Vec<String>,
    triggers: Vec<String>,
    cases: Vec<Case>,
}

/// Everything the vector set's content identity is minted over.
///
/// `content_id` itself is NOT a field here — a value cannot contain its own
/// digest. It is written into the JSON header, and a consumer re-derives it by
/// hashing exactly these fields.
#[derive(Serialize)]
struct Vectors {
    schema: u32,
    grids: Vec<Grid>,
}

impl ContentAddressable for Vectors {
    fn canonical_form(&self) -> Result<Vec<u8>, ContentError> {
        canonical::to_canonical_dagcbor(self)
    }
}

/// The JSON document: the identity header plus the hashed body.
#[derive(Serialize)]
struct Doc {
    /// How to regenerate this file. It is generated; do not hand-edit it.
    generator: &'static str,
    /// The identity of `Vectors` — CIDv1 / dag-cbor / BLAKE3, minted by
    /// `content-addressable`, never by a hand-rolled digest. Each grid carries
    /// its ladder's own id too, so a consumer can pin the table it resolved
    /// against without re-deriving the whole vector set.
    content_id: String,
    #[serde(flatten)]
    vectors: Vectors,
}

fn main() {
    let out_root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".to_string()));

    let demo = Ladder::from_toml(LADDER_TOML).expect("the demo table parses");
    assert_eq!(
        demo.collisions(),
        vec![],
        "the demo table has authoring collisions; fix it before pinning vectors"
    );

    // The hostile table: a rogue rung at the TOP claiming the reserved trigger,
    // plus a private trigger of its own. `zz` is the anti-vacuous half — it is
    // what proves the rogue rung is live and otherwise winning, so that
    // `ctrl-c` reaching the hatch anyway is a statement about resolution order
    // rather than about a rung that never fires.
    let reserved = demo
        .hatch()
        .reserved()
        .next()
        .expect("a hatch is non-empty by construction")
        .to_string();
    let mut rogue_rungs = vec![Rung::new("rogue", [reserved.as_str(), "zz"], "swallow")];
    rogue_rungs.extend(demo.rungs().to_vec());
    let rogue = Ladder::new(
        demo.hatch().clone(),
        rogue_rungs,
        demo.fallthrough().collect::<Vec<_>>(),
    );
    assert_eq!(
        rogue.collisions().len(),
        1,
        "the rogue table must report exactly the rung that claims the hatch"
    );

    let vectors = Vectors {
        schema: SCHEMA,
        grids: vec![
            grid(
                "demo",
                "The demo table, as `spec/vectors/newt_ladder.toml` declares it.",
                demo,
            ),
            grid(
                "rogue",
                "A HOSTILE table: the same rungs with a rogue rung at the top claiming \
                 the reserved trigger. This is the grid that makes resolution order \
                 observable — on the demo table alone, moving the hatch branch below \
                 the rung scan changes no verdict at all.",
                rogue,
            ),
        ],
    };

    let doc = Doc {
        generator: "just gen-vectors",
        content_id: vectors
            .content_id()
            .expect("the vectors encode")
            .to_string(),
        vectors,
    };

    write(
        &out_root.join("spec/vectors/ladder.json"),
        &(serde_json::to_string_pretty(&doc).expect("the vectors serialize") + "\n"),
    );
    write(
        &out_root.join("formal/Precedence/Vectors.lean"),
        &lean(&doc),
    );
}

/// Walk one ladder's whole input space.
fn grid(name: &'static str, note: &'static str, ladder: Ladder) -> Grid {
    let claimants = claimants_of(&ladder);
    assert!(
        claimants.len() <= MAX_CLAIMANTS,
        "{} claimants is too many for a powerset walk (max {MAX_CLAIMANTS})",
        claimants.len()
    );
    let triggers = triggers_of(&ladder);
    let cases = walk(&ladder, &claimants, &triggers);

    // POSITIVE READ ASSERTION. Everything downstream is a comparison against
    // this grid; an empty or truncated grid would make the diff check, the
    // `decide` block and the Python conformance script all pass on nothing.
    assert_eq!(
        cases.len(),
        (1usize << claimants.len()) * triggers.len() * 2,
        "the {name} walk did not cover the whole input space"
    );

    Grid {
        name,
        note,
        ladder_content_id: ladder.content_id().expect("the ladder encodes").to_string(),
        ladder,
        claimants,
        triggers,
        cases,
    }
}

/// The claimant names, deduplicated, in table order.
fn claimants_of(ladder: &Ladder) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in ladder.claimants() {
        if !out.iter().any(|seen| seen == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// Every trigger the table mentions, sorted, plus the unbound probe.
fn triggers_of(ladder: &Ladder) -> Vec<String> {
    let mut named: BTreeSet<String> = BTreeSet::new();
    named.extend(ladder.hatch().reserved().map(str::to_string));
    named.extend(ladder.fallthrough().map(str::to_string));
    for rung in ladder.rungs() {
        named.extend(rung.triggers.iter().cloned());
    }
    assert!(
        !named.contains(UNBOUND_PROBE),
        "the table binds {UNBOUND_PROBE:?}, so it is no longer an unbound probe"
    );
    let mut out: Vec<String> = named.into_iter().collect();
    out.push(UNBOUND_PROBE.to_string());
    out
}

/// Every subset of `claimants` x every trigger x both work states, resolved by
/// the real [`Ladder::resolve`].
fn walk(ladder: &Ladder, claimants: &[String], triggers: &[String]) -> Vec<Case> {
    let mut cases = Vec::new();
    for bits in 0..(1u32 << claimants.len()) {
        let live: Vec<String> = claimants
            .iter()
            .enumerate()
            .filter(|(i, _)| bits & (1 << i) != 0)
            .map(|(_, name)| name.clone())
            .collect();
        let claiming: ClaimSet = live.iter().cloned().collect();
        for trigger in triggers {
            for work_running in [true, false] {
                let s = Situation {
                    claiming: &claiming,
                    work_running,
                };
                cases.push(Case {
                    trigger: trigger.clone(),
                    claiming: live.clone(),
                    work_running,
                    verdict: VerdictRec::of(ladder.resolve(trigger, &s)),
                });
            }
        }
    }
    cases
}

// ---------------------------------------------------------------------------
// Lean rendering
// ---------------------------------------------------------------------------

/// Render the vectors as a Lean module.
///
/// Three theorems per grid, and each rules something out:
///
/// * `*_vectors_agree` — the model reproduces every recorded verdict. This is
///   the transliteration check, and on the `rogue` grid it is also the
///   escape-hatch invariant witnessed against real Rust output rather than
///   proved by construction.
/// * `*_verdict_distribution` — the exact `(claimed, escape, unbound)` counts.
///   A grid that had collapsed to one verdict kind would still satisfy
///   `vectors_agree` (both sides would agree on the collapse); this is what
///   makes that impossible. On the `rogue` grid it is also the numeric witness
///   that the hatch wins: demote the hatch branch and escapes fall as claims
///   rise.
/// * `*_order_is_load_bearing` — the same grid against the same rungs REVERSED
///   gives a different answer vector, so rung ORDER is observable. Without it
///   a ladder would be indistinguishable from a set.
fn lean(doc: &Doc) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "/-\n  GENERATED by `{}` — DO NOT EDIT.\n\n\
         \x20 The golden vectors: every verdict below was produced by the REAL\n\
         \x20 `src/lib.rs::Ladder::resolve`, and is re-derived here by `decide` against\n\
         \x20 the Lean transliteration in `Precedence.Basic`. That agreement is the ONLY\n\
         \x20 thing tying these proofs to the shipped code — the theorems in `Basic.lean`\n\
         \x20 are about the model alone, and nothing in Lean reads `src/lib.rs`.\n\n\
         \x20 `just check-vectors` regenerates this file and fails on any difference, so\n\
         \x20 a hand edit here — or a change to `resolve` without regenerating — is a red\n\
         \x20 build rather than a proof about a table that no longer exists.\n\n\
         \x20 vectors content id: {}\n-/\n\n\
         import Precedence.Basic\n\n\
         -- The `decide` blocks below reduce several hundred cases in one go, which\n\
         -- overflows the elaborator's default recursion budget. Raising it costs\n\
         -- nothing in trust: every proof here is still a KERNEL reduction, and\n\
         -- `#print axioms` on each theorem shows `propext` at most. See\n\
         -- `../README.md` for why the compiled-evaluation tactic is refused\n\
         -- outright rather than used here.\n\
         set_option maxRecDepth 20000\n\n\
         namespace Precedence\n\
         namespace Vectors\n",
        doc.generator, doc.content_id
    ));

    for g in &doc.vectors.grids {
        let stem = g.name;
        out.push_str(&format!("\n/-! ### The `{stem}` grid -/\n\n"));
        out.push_str(&format!(
            "/-- {}\n\n    ladder content id: {} -/\ndef {stem}Ladder : Ladder String String where\n",
            wrap_doc(g.note),
            g.ladder_content_id
        ));
        out.push_str(&format!(
            "  reserved := {}\n  hatchAction := {}\n  rungs := [\n",
            lean_list(g.ladder.hatch().reserved()),
            lean_str(g.ladder.hatch().action())
        ));
        for (i, rung) in g.ladder.rungs().iter().enumerate() {
            out.push_str(&format!(
                "    {{ claimant := {}, triggers := {}, action := {} }}{}\n",
                lean_str(&rung.claimant),
                lean_list(rung.triggers.iter().map(String::as_str)),
                lean_str(&rung.action),
                comma(i, g.ladder.rungs().len())
            ));
        }
        out.push_str(&format!(
            "  ]\n  fallthrough := {}\n\n",
            lean_list(g.ladder.fallthrough())
        ));

        out.push_str(&format!(
            "/-- The same table with its rungs reversed — the permutation\n\
             \x20   `{stem}_order_is_load_bearing` distinguishes. -/\n\
             def {stem}LadderReversed : Ladder String String :=\n\
             \x20 {{ {stem}Ladder with rungs := {stem}Ladder.rungs.reverse }}\n\n\
             /-- Every recorded `(input, verdict)` pair for this grid: {} subsets of\n\
             \x20   {} claimants x {} triggers x 2 work states. -/\n\
             def {stem}Cases : List (Case String String) := [\n",
            1usize << g.claimants.len(),
            g.claimants.len(),
            g.triggers.len()
        ));
        for (i, case) in g.cases.iter().enumerate() {
            out.push_str(&format!(
                "  {{ trigger := {}, claiming := {}, workRunning := {},\n    expected := {} }}{}\n",
                lean_str(&case.trigger),
                lean_list(case.claiming.iter().map(String::as_str)),
                case.work_running,
                lean_verdict(&case.verdict),
                comma(i, g.cases.len())
            ));
        }
        out.push_str("]\n\n");

        let (claimed, escape, unbound) = tally(&g.cases);
        out.push_str(&format!(
            "/-- The model reproduces every verdict the shipped `resolve` produced on\n\
             \x20   this grid. -/\n\
             theorem {stem}_vectors_agree : {stem}Cases.all (caseOk {stem}Ladder) = true := by decide\n\n\
             /-- Non-vacuity: the grid really does produce these exact counts of each\n\
             \x20   verdict kind. `{stem}_vectors_agree` alone would be satisfied by a grid\n\
             \x20   that had collapsed to a single kind. -/\n\
             theorem {stem}_verdict_distribution :\n\
             \x20   tally {stem}Ladder {stem}Cases = ({claimed}, {escape}, {unbound}) := by decide\n\n\
             /-- **A ladder is not a set.** The same {} inputs against the same rungs in\n\
             \x20   the opposite order give a different answer vector, so rung ORDER is\n\
             \x20   observable. This is the deliberate negation of a position-independence\n\
             \x20   law. -/\n\
             theorem {stem}_order_is_load_bearing :\n\
             \x20   outcomes {stem}Ladder {stem}Cases ≠ outcomes {stem}LadderReversed {stem}Cases := by\n\
             \x20 decide\n",
            g.cases.len()
        ));
    }

    out.push_str("\nend Vectors\nend Precedence\n");
    out
}

fn tally(cases: &[Case]) -> (usize, usize, usize) {
    let count = |kind| cases.iter().filter(|c| c.verdict.kind == kind).count();
    (count("claimed"), count("escape"), count("unbound"))
}

fn comma(i: usize, len: usize) -> &'static str {
    if i + 1 == len {
        ""
    } else {
        ","
    }
}

/// Re-wrap a note into an indented Lean doc comment body.
fn wrap_doc(note: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in note.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > 72 {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("\n    ")
}

fn lean_verdict(v: &VerdictRec) -> String {
    match (v.kind, &v.claimant, &v.action) {
        ("claimed", Some(c), Some(a)) => format!(".claimed {} {}", lean_str(c), lean_str(a)),
        ("escape", _, Some(a)) => format!(".escape {}", lean_str(a)),
        ("unbound", _, _) => ".unbound".to_string(),
        other => panic!("unrenderable verdict {other:?}"),
    }
}

fn lean_list<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let rendered: Vec<String> = items.into_iter().map(lean_str).collect();
    format!("[{}]", rendered.join(", "))
}

/// A Lean string literal. Lean's escapes for these two characters are C's, and
/// the demo table's action labels (`cancel [y/N]`, `cancel :`) need neither —
/// but a trigger name is arbitrary consumer text, so escaping is not optional.
fn lean_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the output directory is creatable");
    }
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
    eprintln!("wrote {}", path.display());
}
