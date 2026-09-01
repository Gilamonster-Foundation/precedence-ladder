//! [`Ladder::collisions`] — the authoring lint.
//!
//! A ladder is infallible to construct, because making it fallible would
//! create a validation step a consumer could forget (see the escape-hatch
//! invariant in the crate docs). The cost of that choice is that a table can
//! contain rows that can never fire. This module finds them, so the author
//! learns at load time rather than the operator learning at a keystroke.

use crate::Ladder;
use std::collections::{BTreeMap, BTreeSet};

/// A row of a ladder that can never fire, and why.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Collision {
    /// A rung claims a trigger the hatch reserves.
    ///
    /// While work runs the hatch answers first, so the rung can never win for
    /// that trigger; while work is idle a reserved trigger is `Unbound` and the
    /// rung *can* win, which makes the row's behaviour depend on whether a turn
    /// happens to be running. Almost always an authoring mistake.
    ReservedByHatch {
        /// The rung's claimant.
        claimant: String,
        /// The reserved trigger it also claims.
        trigger: String,
    },
    /// A fallthrough trigger the hatch also reserves.
    ///
    /// The hatch branch returns first, so the fallthrough branch is dead for
    /// that trigger. The verdict is the same either way, which is what makes
    /// this a lint and not a bug — but the row is noise.
    ReservedFallthrough {
        /// The trigger listed in both places.
        trigger: String,
    },
    /// A later rung repeats an earlier rung's `(claimant, trigger)` pair.
    ///
    /// The earlier row answers first for exactly the same condition, so the
    /// later one is dead. Note that two *different* claimants sharing a trigger
    /// is not a collision — that is the whole point of an ordered ladder.
    DeadRung {
        /// The repeated claimant.
        claimant: String,
        /// The repeated trigger.
        trigger: String,
        /// Index of the earlier rung that answers first.
        shadowed_by: usize,
    },
}

impl Ladder {
    /// Report every row of this table that can never fire.
    ///
    /// An authoring lint, not an error: [`Ladder::new`] and
    /// [`Ladder::from_toml`](crate::Ladder::from_toml) both accept a colliding
    /// table, because the escape-hatch invariant already makes such a row
    /// harmless. Call this at load and surface the result to the author.
    ///
    /// ```
    /// use precedence_ladder::{Collision, Hatch, Ladder, Rung};
    /// let l = Ladder::new(
    ///     Hatch::new("interrupt", "ctrl-c", []),
    ///     vec![Rung::new("pager", ["ctrl-c"], "close")],
    ///     ["esc"],
    /// );
    /// assert_eq!(
    ///     l.collisions(),
    ///     vec![Collision::ReservedByHatch {
    ///         claimant: "pager".into(),
    ///         trigger: "ctrl-c".into(),
    ///     }]
    /// );
    /// ```
    pub fn collisions(&self) -> Vec<Collision> {
        let reserved: BTreeSet<&str> = self.hatch().reserved().collect();
        let mut found = Vec::new();
        let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();

        for (index, rung) in self.rungs().iter().enumerate() {
            for trigger in &rung.triggers {
                if reserved.contains(trigger.as_str()) {
                    found.push(Collision::ReservedByHatch {
                        claimant: rung.claimant.clone(),
                        trigger: trigger.clone(),
                    });
                }
                match seen.get(&(rung.claimant.as_str(), trigger.as_str())) {
                    Some(&first) => found.push(Collision::DeadRung {
                        claimant: rung.claimant.clone(),
                        trigger: trigger.clone(),
                        shadowed_by: first,
                    }),
                    None => {
                        seen.insert((rung.claimant.as_str(), trigger.as_str()), index);
                    }
                }
            }
        }

        for trigger in self.fallthrough() {
            if reserved.contains(trigger) {
                found.push(Collision::ReservedFallthrough {
                    trigger: trigger.to_string(),
                });
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClaimSet, Hatch, Rung, Situation, Verdict, NO_FALLTHROUGH};

    fn hatch() -> Hatch {
        Hatch::new("interrupt", "ctrl-c", [])
    }

    #[test]
    fn a_clean_table_has_no_collisions() {
        // ANTI-VACUOUS TWIN for every test below: if `collisions` returned a
        // finding for everything (or the scan read no rows at all and the
        // findings came from somewhere else), this fails.
        let l = Ladder::new(
            hatch(),
            vec![
                Rung::new("palette", ["esc"], "close palette"),
                Rung::new("vi-insert", ["esc"], "NORMAL"),
            ],
            ["esc"],
        );
        assert_eq!(l.collisions(), vec![]);
        assert_eq!(l.rungs().len(), 2, "the scan had rows to look at");
    }

    #[test]
    fn a_rogue_table_is_reported_and_still_resolves_safely() {
        let l = Ladder::new(
            hatch(),
            vec![Rung::new("rogue", ["ctrl-c"], "swallow")],
            ["ctrl-c", "esc"],
        );
        assert_eq!(
            l.collisions(),
            vec![
                Collision::ReservedByHatch {
                    claimant: "rogue".into(),
                    trigger: "ctrl-c".into()
                },
                Collision::ReservedFallthrough {
                    trigger: "ctrl-c".into()
                },
            ]
        );
        // The lint is advisory precisely because the table is still safe.
        let claims: ClaimSet = ["rogue"].into_iter().collect();
        assert_eq!(
            l.resolve(
                "ctrl-c",
                &Situation {
                    claiming: &claims,
                    work_running: true
                }
            ),
            Verdict::Escape {
                action: "interrupt"
            }
        );
    }

    #[test]
    fn a_repeated_claimant_trigger_pair_is_dead() {
        let l = Ladder::new(
            hatch(),
            vec![
                Rung::new("vi", ["esc"], "NORMAL"),
                Rung::new("other", ["esc"], "close"),
                Rung::new("vi", ["esc"], "unreachable"),
            ],
            NO_FALLTHROUGH,
        );
        assert_eq!(
            l.collisions(),
            vec![Collision::DeadRung {
                claimant: "vi".into(),
                trigger: "esc".into(),
                shadowed_by: 0,
            }]
        );
    }

    #[test]
    fn two_claimants_sharing_a_trigger_is_not_a_collision() {
        // The ordered ladder exists for exactly this case; reporting it would
        // make the lint useless noise on every real table.
        let l = Ladder::new(
            hatch(),
            vec![Rung::new("a", ["esc"], "A"), Rung::new("b", ["esc"], "B")],
            NO_FALLTHROUGH,
        );
        assert_eq!(l.collisions(), vec![]);
    }
}
