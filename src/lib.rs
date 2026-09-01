//! Who owns this trigger, right now — and can the operator always get out?
//!
//! A [`Ladder`] is an ordered table of [`Rung`]s, each naming a *claimant* and
//! the triggers it owns, plus a [`Hatch`] whose reserved triggers are the
//! operator's escape. [`Ladder::resolve`] answers, for one trigger in one
//! [`Situation`], which of the three things happened: a claimant owns it, the
//! escape hatch owns it, or nothing is bound.
//!
//! # The escape-hatch invariant
//!
//! > For every ladder `L`, every situation `s` with `s.work_running = true`,
//! > and every trigger `t` in `L`'s reserved set: `resolve(L, t, s) = Escape`.
//!
//! **Unrepresentable, not validated.** The reserved check precedes the rung
//! scan in [`Ladder::resolve`], so no value of `rungs` can affect it — there is
//! no fallible constructor to make and nothing a consumer can forget to call.
//! A rung that claims a reserved trigger is *inert*; [`Ladder::collisions`]
//! reports it as an authoring lint rather than an error.
//!
//! Non-emptiness of the reserved set — the thing that keeps the statement from
//! being vacuous — is by construction too: [`Hatch::new`] takes a *first*
//! element plus a *rest*, so an empty reserved set cannot be spelled.
//!
//! **Scope.** This is a property of *one ladder*. It is not the claim "the
//! operator can always get out of the application"; that needs every input
//! context to have a ladder, and is discharged in the consumer by a
//! registration ratchet, not here.
//!
//! # What this crate is not
//!
//! It decodes no keys, owns no terminal, renders nothing, and dispatches
//! nothing. A trigger is an opaque string, so no terminal type appears in any
//! signature. The whole API is plain values, which is what lets it cross the
//! PyO3 and wasm boundaries unchanged.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeSet;

#[cfg(feature = "cid")]
mod cid;
mod collide;
#[cfg(feature = "table")]
mod table;

pub use collide::Collision;
#[cfg(feature = "table")]
pub use table::{LadderError, SCHEMA};

/// The operator's escape hatch: the triggers that always reach it while work
/// is running, and the action label the consumer shows for them.
///
/// Non-empty by construction. There is no `Default`, no `new(Vec)`, and no
/// `Result`: an empty reserved set would make the escape-hatch invariant
/// vacuous, so it is not expressible.
///
/// # Why there is no `Deserialize`
///
/// Deriving `Deserialize` would reopen exactly the hole [`Hatch::new`] closes —
/// a wire form carrying `reserved = []` would mint an empty hatch without ever
/// calling the constructor. [`Ladder::from_toml`] therefore parses into a local
/// wire type and routes through [`Hatch::new`], which is why a table with an
/// empty `reserved` is a parse *error* rather than a vacuous ladder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Hatch {
    reserved: BTreeSet<String>,
    action: String,
}

impl Hatch {
    /// Build a hatch from an action label and a **non-empty** trigger set.
    ///
    /// ```
    /// use precedence_ladder::Hatch;
    /// let one = Hatch::new("interrupt", "ctrl-c", []);
    /// let two = Hatch::new("interrupt", "ctrl-c", ["ctrl-\\".to_string()]);
    /// assert_eq!(one.reserved().count(), 1);
    /// assert_eq!(two.reserved().count(), 2);
    /// ```
    pub fn new(
        action: impl Into<String>,
        first: impl Into<String>,
        rest: impl IntoIterator<Item = String>,
    ) -> Hatch {
        let mut reserved = BTreeSet::new();
        reserved.insert(first.into());
        reserved.extend(rest);
        Hatch {
            reserved,
            action: action.into(),
        }
    }

    /// The reserved triggers, in sorted order. Never empty.
    pub fn reserved(&self) -> impl Iterator<Item = &str> {
        self.reserved.iter().map(String::as_str)
    }

    /// The action label the consumer shows for a reserved trigger.
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// One row of the table: a named claimant, the triggers it owns while it is
/// live, and the action label for them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Rung {
    /// The claimant's name. Matched against [`ClaimSet`] by string equality.
    pub claimant: String,
    /// The triggers this rung owns.
    pub triggers: BTreeSet<String>,
    /// What the consumer should say this rung's triggers do.
    pub action: String,
}

impl Rung {
    /// Build a rung.
    ///
    /// ```
    /// use precedence_ladder::Rung;
    /// let r = Rung::new("vi-insert", ["esc"], "NORMAL");
    /// assert_eq!(r.claimant, "vi-insert");
    /// ```
    pub fn new<T>(claimant: impl Into<String>, triggers: T, action: impl Into<String>) -> Rung
    where
        T: IntoIterator,
        T::Item: Into<String>,
    {
        Rung {
            claimant: claimant.into(),
            triggers: triggers.into_iter().map(Into::into).collect(),
            action: action.into(),
        }
    }
}

/// An ordered precedence table. Build it with [`Ladder::new`] or, preferably,
/// load it from TOML with [`Ladder::from_toml`] so the knowledge lives in data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Ladder {
    hatch: Hatch,
    rungs: Vec<Rung>,
    fallthrough: BTreeSet<String>,
}

/// The empty fallthrough set, spelled so inference works.
///
/// `Ladder::new(hatch, rungs, [])` cannot compile: the element type of a bare
/// empty array is unconstrained. Pass this instead — a ladder whose declined
/// triggers reach nothing.
///
/// ```
/// use precedence_ladder::{Hatch, Ladder, NO_FALLTHROUGH};
/// let l = Ladder::new(Hatch::new("interrupt", "ctrl-c", []), vec![], NO_FALLTHROUGH);
/// assert_eq!(l.fallthrough().count(), 0);
/// ```
pub const NO_FALLTHROUGH: [&str; 0] = [];

/// Which claimant **names** are claiming right now.
///
/// A value, never a trait object or a callback: that is what lets the whole
/// API cross PyO3 and wasm as plain data, and it is why a claimant cannot
/// re-enter consumer state in the middle of a decision.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClaimSet(BTreeSet<String>);

impl ClaimSet {
    /// Record that `name` is claiming. Chainable.
    ///
    /// ```
    /// use precedence_ladder::ClaimSet;
    /// let mut c = ClaimSet::default();
    /// c.claiming("palette").claiming("vi-insert");
    /// assert!(c.is_live("palette"));
    /// ```
    pub fn claiming(&mut self, name: &str) -> &mut Self {
        self.0.insert(name.to_string());
        self
    }

    /// Is `name` claiming?
    pub fn is_live(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    /// The claiming names, in sorted order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

impl<S: Into<String>> FromIterator<S> for ClaimSet {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        ClaimSet(iter.into_iter().map(Into::into).collect())
    }
}

/// Everything [`Ladder::resolve`] is allowed to know about the moment.
///
/// Deliberately two fields. Anything richer would pull consumer state into the
/// decision, and the crate would stop being a pure predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Situation<'a> {
    /// Which claimants are live.
    pub claiming: &'a ClaimSet,
    /// Is a unit of work running?
    ///
    /// **CEILING: one flat work unit.** A harness with a tool call nested
    /// inside a turn has no single answer here and must NOT pass `true`
    /// unconditionally — [`Ladder::describe`] would then advertise an
    /// interrupt that the outer unit will not perform. If you need nesting,
    /// keep a depth counter in the consumer and pass `depth > 0`.
    pub work_running: bool,
}

/// What a trigger means, in one situation, according to one ladder.
///
/// Borrows its labels from the ladder, so a binding that crosses an FFI
/// boundary clones them there — one small clone per keystroke, free at human
/// timescales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict<'l> {
    /// A live claimant owns the trigger.
    Claimed {
        /// The owning claimant's name.
        claimant: &'l str,
        /// What it does.
        action: &'l str,
    },
    /// The trigger reached the operator's escape hatch.
    Escape {
        /// What the hatch does.
        action: &'l str,
    },
    /// Nothing is bound; the consumer's default handling applies.
    Unbound,
}

impl<'l> Verdict<'l> {
    /// The action label, if the trigger does anything at all.
    pub fn action(&self) -> Option<&'l str> {
        match *self {
            Verdict::Claimed { action, .. } | Verdict::Escape { action } => Some(action),
            Verdict::Unbound => None,
        }
    }
}

impl Ladder {
    /// Build a ladder. **Infallible by design.**
    ///
    /// A rung naming a reserved trigger is inert by resolution order, so there
    /// is nothing here to reject; [`Ladder::collisions`] reports such a row as
    /// an authoring lint. Making this fallible would create a validation step a
    /// consumer could forget, which is exactly what the escape-hatch invariant
    /// is designed not to need.
    pub fn new<F>(hatch: Hatch, rungs: Vec<Rung>, fallthrough: F) -> Ladder
    where
        F: IntoIterator,
        F::Item: Into<String>,
    {
        Ladder {
            hatch,
            rungs,
            fallthrough: fallthrough.into_iter().map(Into::into).collect(),
        }
    }

    /// Resolve one trigger in one situation. First match wins.
    ///
    /// The order of the three branches *is* the contract:
    ///
    /// 1. **The hatch, while work runs.** No rung is reachable here — this is
    ///    the escape-hatch invariant, and it holds for every possible value of
    ///    `rungs`.
    /// 2. **The rungs, in table order**, restricted to live claimants.
    /// 3. **Fallthrough, while work runs** — the trigger every claimant
    ///    declined still reaches the hatch's action.
    ///
    /// Anything else is [`Verdict::Unbound`]. In particular a *reserved*
    /// trigger while idle is `Unbound`, not `Escape`: there is no work to
    /// escape from, and the consumer's idle handling (clearing a draft, say)
    /// is policy this crate does not own.
    pub fn resolve<'l>(&'l self, trigger: &str, s: &Situation<'_>) -> Verdict<'l> {
        if s.work_running && self.hatch.reserved.contains(trigger) {
            // No rung is reachable here. This line, and its position above the
            // loop, is the whole escape-hatch invariant.
            return Verdict::Escape {
                action: &self.hatch.action,
            };
        }
        for rung in &self.rungs {
            if rung.triggers.contains(trigger) && s.claiming.is_live(&rung.claimant) {
                return Verdict::Claimed {
                    claimant: &rung.claimant,
                    action: &rung.action,
                };
            }
        }
        if s.work_running && self.fallthrough.contains(trigger) {
            return Verdict::Escape {
                action: &self.hatch.action,
            };
        }
        Verdict::Unbound
    }

    /// What `trigger` **would** do, from the same table that decides it.
    ///
    /// This is the affordance half of the contract: a hint rendered from
    /// `describe` cannot advertise something `resolve` will not do, because
    /// there is only one table and one traversal. Returns a label borrowed from
    /// the ladder — never a crate constant — so the vocabulary belongs to the
    /// consumer and is localizable.
    ///
    /// ```
    /// use precedence_ladder::{ClaimSet, Hatch, Ladder, Situation};
    /// let l = Ladder::new(Hatch::new("interrupt", "ctrl-c", []), vec![], ["esc"]);
    /// let idle = ClaimSet::default();
    /// // Idle Ctrl-C escapes nothing, so it advertises nothing.
    /// assert_eq!(l.describe("ctrl-c", &Situation { claiming: &idle, work_running: false }), None);
    /// assert_eq!(
    ///     l.describe("ctrl-c", &Situation { claiming: &idle, work_running: true }),
    ///     Some("interrupt")
    /// );
    /// ```
    pub fn describe(&self, trigger: &str, s: &Situation<'_>) -> Option<&str> {
        self.resolve(trigger, s).action()
    }

    /// The claimant names this ladder mentions, in table order.
    ///
    /// Drives the registration conformance test in the consumer: every name
    /// here must be answerable by the consumer's claim accessors, or a rung
    /// added to the table can never fire. Not deduplicated — one claimant may
    /// legitimately own several triggers on several rows.
    pub fn claimants(&self) -> impl Iterator<Item = &str> {
        self.rungs.iter().map(|r| r.claimant.as_str())
    }

    /// The hatch.
    pub fn hatch(&self) -> &Hatch {
        &self.hatch
    }

    /// The rungs, in table order.
    pub fn rungs(&self) -> &[Rung] {
        &self.rungs
    }

    /// The fallthrough triggers, in sorted order.
    pub fn fallthrough(&self) -> impl Iterator<Item = &str> {
        self.fallthrough.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hatch() -> Hatch {
        Hatch::new("interrupt", "ctrl-c", [])
    }

    #[test]
    fn a_hatch_cannot_be_empty() {
        // Not an assertion about a runtime check — an assertion that the only
        // door takes a `first`, so the empty set has no spelling. If this ever
        // reads `Hatch::new(action, [])`, the invariant became vacuous.
        assert_eq!(hatch().reserved().collect::<Vec<_>>(), vec!["ctrl-c"]);
    }

    #[test]
    fn a_rung_claiming_a_reserved_trigger_is_inert_while_work_runs() {
        let rogue = Rung::new("rogue", ["ctrl-c", "zz"], "swallow");
        let l = Ladder::new(hatch(), vec![rogue], ["esc"]);
        let claims: ClaimSet = ["rogue"].into_iter().collect();
        let running = Situation {
            claiming: &claims,
            work_running: true,
        };

        assert_eq!(
            l.resolve("ctrl-c", &running),
            Verdict::Escape {
                action: "interrupt"
            }
        );

        // ANTI-VACUOUS TWIN: the rogue rung must actually be live and
        // otherwise-winning, or the assertion above passes for the wrong
        // reason (a rung that never fires proves nothing about precedence).
        assert_eq!(
            l.resolve("zz", &running),
            Verdict::Claimed {
                claimant: "rogue",
                action: "swallow"
            }
        );
    }

    #[test]
    fn a_reserved_trigger_while_idle_is_unbound() {
        let l = Ladder::new(hatch(), vec![], ["esc"]);
        let none = ClaimSet::default();
        let idle = Situation {
            claiming: &none,
            work_running: false,
        };
        assert_eq!(l.resolve("ctrl-c", &idle), Verdict::Unbound);
        assert_eq!(l.describe("ctrl-c", &idle), None);
    }

    #[test]
    fn order_is_load_bearing() {
        // Without this the ladder is a set, not a ladder.
        let a = Rung::new("a", ["esc"], "A");
        let b = Rung::new("b", ["esc"], "B");
        let claims: ClaimSet = ["a", "b"].into_iter().collect();
        let s = Situation {
            claiming: &claims,
            work_running: true,
        };
        let ab = Ladder::new(hatch(), vec![a.clone(), b.clone()], ["esc"]);
        let ba = Ladder::new(hatch(), vec![b, a], ["esc"]);
        assert_eq!(
            ab.resolve("esc", &s),
            Verdict::Claimed {
                claimant: "a",
                action: "A"
            }
        );
        assert_eq!(
            ba.resolve("esc", &s),
            Verdict::Claimed {
                claimant: "b",
                action: "B"
            }
        );
    }

    #[test]
    fn fallthrough_only_fires_when_every_claimant_declined() {
        let l = Ladder::new(hatch(), vec![Rung::new("vi", ["esc"], "NORMAL")], ["esc"]);
        let live: ClaimSet = ["vi"].into_iter().collect();
        let none = ClaimSet::default();
        assert_eq!(
            l.resolve(
                "esc",
                &Situation {
                    claiming: &live,
                    work_running: true
                }
            ),
            Verdict::Claimed {
                claimant: "vi",
                action: "NORMAL"
            }
        );
        assert_eq!(
            l.resolve(
                "esc",
                &Situation {
                    claiming: &none,
                    work_running: true
                }
            ),
            Verdict::Escape {
                action: "interrupt"
            }
        );
        // ANTI-VACUOUS TWIN: idle, the same declined Esc must NOT escape —
        // otherwise the `work_running` conjunct in the fallthrough branch is
        // dead and the test above would pass without it.
        assert_eq!(
            l.resolve(
                "esc",
                &Situation {
                    claiming: &none,
                    work_running: false
                }
            ),
            Verdict::Unbound
        );
    }

    #[test]
    fn an_unregistered_claimant_never_wins() {
        let l = Ladder::new(
            hatch(),
            vec![Rung::new("vi", ["esc"], "NORMAL")],
            NO_FALLTHROUGH,
        );
        let other: ClaimSet = ["palette"].into_iter().collect();
        assert_eq!(
            l.resolve(
                "esc",
                &Situation {
                    claiming: &other,
                    work_running: true
                }
            ),
            Verdict::Unbound
        );
    }

    #[test]
    fn claimants_lists_the_table_in_order() {
        let l = Ladder::new(
            hatch(),
            vec![Rung::new("a", ["esc"], "A"), Rung::new("b", ["x"], "B")],
            NO_FALLTHROUGH,
        );
        assert_eq!(l.claimants().collect::<Vec<_>>(), vec!["a", "b"]);
    }
}
