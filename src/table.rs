//! [`Ladder::from_toml`] — the ladder as data.
//!
//! Knowledge belongs in data, not logic: a new claimant is a table row, not a
//! new arm of a hardcoded `match`. This module is the only part of the crate
//! that parses anything, and it is behind the default `table` feature so the
//! core predicate keeps an empty dependency closure.

use crate::{Hatch, Ladder, Rung};
use std::collections::BTreeSet;
use std::fmt;

/// The only table schema this version understands.
///
/// A table declaring anything else is rejected rather than guessed at: an
/// unrecognized future schema is far more likely to mean something new than to
/// mean what this version would do with it.
pub const SCHEMA: u32 = 1;

/// Why a TOML table is not a ladder.
#[derive(Debug)]
pub enum LadderError {
    /// The source is not well-formed TOML, or does not have the table's shape.
    ///
    /// Includes unknown keys: every wire struct is `deny_unknown_fields`, so a
    /// typo'd key is an error rather than a silently ignored row.
    Parse(toml::de::Error),
    /// The table declares a schema this version does not understand.
    Schema {
        /// What the table declared.
        found: u32,
        /// What this version understands ([`SCHEMA`]).
        expected: u32,
    },
    /// `[hatch] reserved` was empty.
    ///
    /// The escape-hatch invariant is vacuous over an empty reserved set, and
    /// [`Hatch`] cannot represent one, so there is nothing to build. This is
    /// the one place non-emptiness is *checked* rather than *unrepresentable* —
    /// and it is a parse boundary, so the check happens exactly once, at the
    /// only door untyped data comes through.
    EmptyReserved,
}

impl fmt::Display for LadderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LadderError::Parse(e) => write!(f, "the ladder table is not well-formed: {e}"),
            LadderError::Schema { found, expected } => write!(
                f,
                "the ladder table declares schema {found}, but this version \
                 understands schema {expected}"
            ),
            LadderError::EmptyReserved => f.write_str(
                "[hatch] reserved is empty: a hatch with no reserved trigger \
                 makes the escape-hatch invariant vacuous, so it cannot be built",
            ),
        }
    }
}

impl std::error::Error for LadderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LadderError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

/// The wire shape of a whole table.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    schema: u32,
    #[serde(default)]
    fallthrough: BTreeSet<String>,
    hatch: HatchDoc,
    #[serde(default)]
    rung: Vec<Rung>,
}

/// The wire shape of a hatch.
///
/// Deliberately **not** `Hatch` itself: deriving `Deserialize` on `Hatch` would
/// let a table with `reserved = []` mint an empty hatch without ever calling
/// [`Hatch::new`], reopening the exact hole the constructor closes. Parsing into
/// this and routing through the constructor is what makes an empty reserved set
/// a parse error instead of a vacuous invariant.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HatchDoc {
    action: String,
    reserved: Vec<String>,
}

impl Ladder {
    /// Parse a ladder from TOML.
    ///
    /// ```
    /// use precedence_ladder::{ClaimSet, Ladder, Situation, Verdict};
    /// let l = Ladder::from_toml(r#"
    ///     schema = 1
    ///     fallthrough = ["esc"]
    ///
    ///     [hatch]
    ///     action = "interrupt"
    ///     reserved = ["ctrl-c"]
    ///
    ///     [[rung]]
    ///     claimant = "vi-insert"
    ///     triggers = ["esc"]
    ///     action = "NORMAL"
    /// "#).unwrap();
    ///
    /// let claims: ClaimSet = ["vi-insert"].into_iter().collect();
    /// let s = Situation { claiming: &claims, work_running: true };
    /// assert_eq!(l.resolve("esc", &s), Verdict::Claimed { claimant: "vi-insert", action: "NORMAL" });
    /// assert_eq!(l.resolve("ctrl-c", &s), Verdict::Escape { action: "interrupt" });
    /// ```
    ///
    /// # Errors
    ///
    /// [`LadderError`] if the source is not well-formed TOML, carries an
    /// unknown key, declares a schema other than [`SCHEMA`], or leaves
    /// `[hatch] reserved` empty.
    pub fn from_toml(src: &str) -> Result<Ladder, LadderError> {
        let doc: Doc = toml::from_str(src).map_err(LadderError::Parse)?;
        if doc.schema != SCHEMA {
            return Err(LadderError::Schema {
                found: doc.schema,
                expected: SCHEMA,
            });
        }
        let mut reserved = doc.hatch.reserved.into_iter();
        let first = reserved.next().ok_or(LadderError::EmptyReserved)?;
        Ok(Ladder::new(
            Hatch::new(doc.hatch.action, first, reserved),
            doc.rung,
            doc.fallthrough,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
schema = 1
fallthrough = ["esc"]

[hatch]
action = "interrupt"
reserved = ["ctrl-c"]

[[rung]]
claimant = "palette"
triggers = ["esc"]
action = "close palette"
"#;

    #[test]
    fn a_table_becomes_a_ladder() {
        let l = Ladder::from_toml(GOOD).expect("parses");
        assert_eq!(l.hatch().reserved().collect::<Vec<_>>(), vec!["ctrl-c"]);
        assert_eq!(l.hatch().action(), "interrupt");
        assert_eq!(l.claimants().collect::<Vec<_>>(), vec!["palette"]);
        assert_eq!(l.fallthrough().collect::<Vec<_>>(), vec!["esc"]);
    }

    #[test]
    fn an_empty_reserved_set_is_a_parse_error() {
        let src = GOOD.replace(r#"reserved = ["ctrl-c"]"#, "reserved = []");
        assert!(matches!(
            Ladder::from_toml(&src),
            Err(LadderError::EmptyReserved)
        ));
        // ANTI-VACUOUS TWIN: the substitution must actually have happened, or
        // this test is asserting against the unmodified GOOD table (which
        // parses) and would fail for the right reason by accident.
        assert!(
            src.contains("reserved = []"),
            "the fixture was not modified"
        );
    }

    #[test]
    fn an_unknown_schema_is_refused() {
        let src = GOOD.replace("schema = 1", "schema = 2");
        assert!(matches!(
            Ladder::from_toml(&src),
            Err(LadderError::Schema {
                found: 2,
                expected: 1
            })
        ));
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        // A silently ignored key is how a table row stops firing without
        // anyone noticing.
        for src in [
            format!("{GOOD}\nwhoops = true\n"),
            GOOD.replace("action = \"close palette\"", "actoin = \"close palette\""),
            GOOD.replace(
                "action = \"interrupt\"",
                "action = \"interrupt\"\nextra = 1",
            ),
        ] {
            assert!(
                matches!(Ladder::from_toml(&src), Err(LadderError::Parse(_))),
                "an unknown key was accepted in:\n{src}"
            );
        }
    }

    #[test]
    fn a_table_with_no_rungs_is_legal() {
        // The minimum useful ladder: a hatch and nothing else. A consumer
        // converting its first input context has exactly this.
        let src = "schema = 1\n[hatch]\naction = \"interrupt\"\nreserved = [\"ctrl-c\"]\n";
        let l = Ladder::from_toml(src).expect("parses");
        assert_eq!(l.rungs().len(), 0);
        assert_eq!(l.fallthrough().count(), 0);
    }

    #[test]
    fn the_error_display_names_the_problem() {
        let src = GOOD.replace(r#"reserved = ["ctrl-c"]"#, "reserved = []");
        let e = Ladder::from_toml(&src).expect_err("empty reserved");
        assert!(e.to_string().contains("vacuous"), "unhelpful: {e}");
    }
}
