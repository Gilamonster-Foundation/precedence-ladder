//! A ladder's content identity, behind the off-by-default `cid` feature.
//!
//! The identity is minted by the `content-addressable` crate — a CIDv1 over
//! canonical DAG-CBOR with a BLAKE3 digest. **Never a hand-rolled digest.** A
//! bespoke canonical encoding is a defect in this line, not a style choice: in
//! this workspace every hand-rolled canonicalization has had a flaw found by
//! review, and every identity minted through the crate has had none.
//!
//! The feature is off by default because `content-addressable` pulls BLAKE3 and
//! the multiformats stack, which would make a wasm build of a pure predicate an
//! order of magnitude larger than it has any business being.
//!
//! # What the id is over
//!
//! The whole ladder, in table order: the hatch's reserved set and action, every
//! rung's claimant/triggers/action, and the fallthrough set. **Rung order is
//! part of the identity**, because it is part of the meaning — a ladder is not
//! a set, and two tables that differ only in row order resolve differently.
//!
//! These bytes are NOT frozen: `0.1.0` is unpublished, and the golden vectors
//! that pin them land in slice C2.

use crate::Ladder;
use content_addressable::{canonical, ContentAddressable, ContentError};

impl ContentAddressable for Ladder {
    fn canonical_form(&self) -> Result<Vec<u8>, ContentError> {
        canonical::to_canonical_dagcbor(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Hatch, Rung};

    fn ladder(rungs: Vec<Rung>) -> Ladder {
        Ladder::new(Hatch::new("interrupt", "ctrl-c", []), rungs, ["esc"])
    }

    fn a() -> Rung {
        Rung::new("palette", ["esc"], "close palette")
    }
    fn b() -> Rung {
        Rung::new("vi-insert", ["esc"], "NORMAL")
    }

    #[test]
    fn the_same_table_has_the_same_id() {
        let id = ladder(vec![a(), b()]).content_id().expect("encodes");
        assert_eq!(id, ladder(vec![a(), b()]).content_id().expect("encodes"));
        assert!(ladder(vec![a(), b()]).verify(&id).expect("verifies"));
    }

    #[test]
    fn rung_order_changes_the_id() {
        // ANTI-VACUOUS TWIN for the test above: if the id were a constant (or
        // computed over a field set that omits the rungs), the equality test
        // would pass and mean nothing. Order is part of the meaning, so it must
        // be part of the identity.
        assert_ne!(
            ladder(vec![a(), b()]).content_id().expect("encodes"),
            ladder(vec![b(), a()]).content_id().expect("encodes"),
        );
    }

    #[test]
    fn every_field_is_inside_the_identity() {
        let base = ladder(vec![a()]).content_id().expect("encodes");
        let variants = [
            // a different hatch action
            Ladder::new(Hatch::new("stop", "ctrl-c", []), vec![a()], ["esc"]),
            // a different reserved trigger
            Ladder::new(Hatch::new("interrupt", "ctrl-d", []), vec![a()], ["esc"]),
            // a second reserved trigger
            Ladder::new(
                Hatch::new("interrupt", "ctrl-c", ["ctrl-d".to_string()]),
                vec![a()],
                ["esc"],
            ),
            // a different rung action
            Ladder::new(
                Hatch::new("interrupt", "ctrl-c", []),
                vec![Rung::new("palette", ["esc"], "dismiss")],
                ["esc"],
            ),
            // a different rung trigger
            Ladder::new(
                Hatch::new("interrupt", "ctrl-c", []),
                vec![Rung::new("palette", ["q"], "close palette")],
                ["esc"],
            ),
            // a different claimant
            Ladder::new(
                Hatch::new("interrupt", "ctrl-c", []),
                vec![Rung::new("pager", ["esc"], "close palette")],
                ["esc"],
            ),
            // a different fallthrough set
            Ladder::new(Hatch::new("interrupt", "ctrl-c", []), vec![a()], ["q"]),
        ];
        for (i, v) in variants.iter().enumerate() {
            assert_ne!(
                base,
                v.content_id().expect("encodes"),
                "variant {i} minted the same id as the base table — a field \
                 that changes the ladder's meaning is outside its identity"
            );
        }
    }
}
