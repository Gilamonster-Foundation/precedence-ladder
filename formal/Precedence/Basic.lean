/-
  Precedence.Basic — the machine-checked model of `Ladder::resolve`.

  This is a TRANSLITERATION of `src/lib.rs::Ladder::resolve`: four branches in
  source order — the hatch while work runs, the rung scan (first match), the
  fallthrough while work runs, and `Unbound`. Nothing else about the crate is
  modelled, because nothing else has an algebra worth proving.

  READ `../README.md` BEFORE CITING ANY THEOREM HERE. Two of the four are true
  *by construction of this model* and prove that the model is consistent, not
  that the Rust refines it. The README says which, and what each one does not
  establish. A theorem quoted without that qualifier is a decorative artifact.

  Modelled in the idiom of newt-agent's `formal/NewtInteraction/Binding.lean`:
  self-contained (no Mathlib), everything `decide`-able, polymorphic in the
  trigger and claimant types so the model cannot smuggle in a terminal.

  MODEL GAP, stated rather than implied: Rust uses `BTreeSet` for `reserved`,
  `triggers`, `fallthrough` and the claim set; this uses `List`. `resolve` only
  ever asks those four for MEMBERSHIP, so the verdict is identical — but a
  property that depended on sortedness or deduplication would NOT transfer, and
  none is proven here.
-/

namespace Precedence

/-- One row of the table: a named claimant, the triggers it owns while it is
    live, and the action label for them (`src/lib.rs::Rung`). -/
structure Rung (T C : Type) where
  /-- The claimant's name. -/
  claimant : C
  /-- The triggers this rung owns. -/
  triggers : List T
  /-- What the consumer should say this rung's triggers do. -/
  action : String
  deriving Repr, DecidableEq, BEq

/-- An ordered precedence table (`src/lib.rs::Ladder` plus its `Hatch`).

    The hatch is inlined as `reserved` + `hatchAction` rather than modelled as
    its own structure: `Hatch`'s only content in Rust is the non-emptiness that
    `Hatch::new` makes unrepresentable, and a Lean structure cannot express
    "there is no other constructor". Non-emptiness is therefore NOT modelled
    here — see the README's note on `resolve_hatch`. -/
structure Ladder (T C : Type) where
  /-- The triggers that always reach the operator's escape while work runs. -/
  reserved : List T
  /-- The action label the hatch's triggers carry. -/
  hatchAction : String
  /-- The rungs, in table order. Order is part of the meaning. -/
  rungs : List (Rung T C)
  /-- Triggers that reach the hatch once every claimant has declined. -/
  fallthrough : List T
  deriving Repr, DecidableEq, BEq

/-- Everything `resolve` is allowed to know about the moment
    (`src/lib.rs::Situation`). -/
structure Situation (C : Type) where
  /-- Which claimants are live. -/
  claiming : List C
  /-- Is a unit of work running? -/
  workRunning : Bool
  deriving Repr, DecidableEq, BEq

/-- What a trigger means, in one situation, according to one ladder
    (`src/lib.rs::Verdict`). -/
inductive Verdict (C : Type) where
  /-- A live claimant owns the trigger. -/
  | claimed (claimant : C) (action : String)
  /-- The trigger reached the operator's escape hatch. -/
  | escape (action : String)
  /-- Nothing is bound. -/
  | unbound
  deriving Repr, DecidableEq, BEq

/-- The action label, if the trigger does anything at all
    (`Verdict::action`). -/
def Verdict.action {C : Type} : Verdict C → Option String
  | .claimed _ a => some a
  | .escape a => some a
  | .unbound => none

variable {T C : Type}

/-- The rung-scan predicate: this rung owns `t` AND its claimant is live. The
    conjunct inside the Rust `for` loop, named so the theorems below can talk
    about it. -/
def rungMatches [BEq T] [BEq C] (t : T) (s : Situation C) (r : Rung T C) : Bool :=
  r.triggers.contains t && s.claiming.contains r.claimant

/-- The rung scan itself: FIRST match wins. A direct transliteration of the
    `for rung in &self.rungs { … return … }` loop — the early `return` is the
    `then` branch, so "first" is structural, not an extra condition. -/
def firstClaim [BEq T] [BEq C] (rungs : List (Rung T C)) (t : T) (s : Situation C) :
    Option (Rung T C) :=
  match rungs with
  | [] => none
  | r :: rest => if rungMatches t s r then some r else firstClaim rest t s

/-- `Ladder::resolve`, transliterated. The order of the three branches IS the
    contract; see the crate docs. -/
def resolve [BEq T] [BEq C] (L : Ladder T C) (t : T) (s : Situation C) : Verdict C :=
  if s.workRunning && L.reserved.contains t then
    -- No rung is reachable here. This branch, and its position above the scan,
    -- is the whole escape-hatch invariant.
    Verdict.escape L.hatchAction
  else
    match firstClaim L.rungs t s with
    | some r => Verdict.claimed r.claimant r.action
    | none =>
      if s.workRunning && L.fallthrough.contains t then
        Verdict.escape L.hatchAction
      else
        Verdict.unbound

/-- `Ladder::describe`, transliterated: the SAME traversal, read for its label.
    Definitional in Rust too (`self.resolve(trigger, s).action()`). -/
def describe [BEq T] [BEq C] (L : Ladder T C) (t : T) (s : Situation C) : Option String :=
  (resolve L t s).action

/-! ## The theorems

    Four, and the README states for each whether it constrains the Rust or only
    the model. Anything that constrained nothing was cut rather than kept for
    the count. -/

/-- **The escape-hatch invariant.** A reserved trigger, while work runs,
    escapes — for EVERY ladder, so no value of `rungs` can affect it.

    STATUS: `spec` (model-internal, true by construction). Discharged by `simp`
    on this file's own `resolve`, because the hatch branch precedes the rung
    scan *in this definition*. It establishes that the model is consistent with
    the property the crate advertises. It does NOT establish that Rust's
    `resolve` has the same branch order — nothing in Lean reads `src/lib.rs`.
    The evidence for the Rust is `tests/truth_table.rs`, which checks the
    invariant at all 192 points of the grid, including hostile tables.

    What the theorem is worth anyway: it fixes the *shape* of the design claim
    (unconditional in `rungs`, no hypothesis about well-formedness), which is
    exactly the shape an earlier design got wrong — a `wf`-hypothesis version
    of this statement is FALSE under hatch-first resolution. -/
theorem resolve_hatch [BEq T] [BEq C] (L : Ladder T C) (t : T) (s : Situation C)
    (hres : L.reserved.contains t = true) (hwork : s.workRunning = true) :
    resolve L t s = Verdict.escape L.hatchAction := by
  simp [resolve, hres, hwork]

/-- **Affordance and behaviour share one table.** `describe` is `resolve`'s
    label, so a hint can never advertise something `resolve` will not do.

    STATUS: `spec` (`rfl`). Definitional in Rust too, which is the point: the
    agreement is not maintained, it is unrepresentable-to-break as long as
    `describe` is written this way. It does NOT establish that any consumer
    renders its hint FROM `describe` — that is a consumer-side test. -/
theorem describe_agrees [BEq T] [BEq C] (L : Ladder T C) (t : T) (s : Situation C) :
    describe L t s = (resolve L t s).action := rfl

/-- The scan's inversion lemma: a match splits the rung list, and everything
    before the winner declined. The "nothing earlier claimed" half is the
    content; the split is what makes it usable. -/
theorem firstClaim_inv [BEq T] [BEq C] {rungs : List (Rung T C)} {t : T} {s : Situation C}
    {r : Rung T C} (h : firstClaim rungs t s = some r) :
    rungMatches t s r = true ∧
      ∃ pre post, rungs = pre ++ r :: post ∧ ∀ q ∈ pre, rungMatches t s q = false := by
  induction rungs with
  | nil => simp [firstClaim] at h
  | cons a rest ih =>
    rw [firstClaim] at h
    split at h
    · next hm =>
      have : a = r := by simpa using h
      subst this
      exact ⟨hm, [], rest, rfl, by simp⟩
    · next hm =>
      obtain ⟨hmatch, pre, post, heq, hpre⟩ := ih h
      refine ⟨hmatch, a :: pre, post, by simp [heq], ?_⟩
      intro q hq
      rcases List.mem_cons.mp hq with hqa | hq'
      · subst hqa; simpa using hm
      · exact hpre q hq'

/-- **First-match determinism.** A `claimed` verdict names a rung that really
    is in the table, really does own the trigger, really is live — and EVERY
    rung ahead of it declined. It also implies the hatch branch did not fire,
    which is what stops "first match" from being a claim about a scan the
    reserved branch already short-circuited.

    STATUS: `proven`. Not `rfl`, not `simp` on the definition: the
    "everything earlier declined" half needs the induction in
    `firstClaim_inv`. It is a statement about this model's `resolve`; it does
    NOT establish that Rust's loop is this scan. Its Rust twin is
    `tests/truth_table.rs::the_whole_input_space`, whose per-claimant win
    counts (32/16/8/4/2) are exactly this lemma's numerical shadow. -/
theorem first_match_inv [BEq T] [BEq C] {L : Ladder T C} {t : T} {s : Situation C}
    {c : C} {a : String} (h : resolve L t s = Verdict.claimed c a) :
    (s.workRunning && L.reserved.contains t) = false ∧
      ∃ pre r post, L.rungs = pre ++ r :: post
        ∧ r.claimant = c ∧ r.action = a
        ∧ rungMatches t s r = true
        ∧ ∀ q ∈ pre, rungMatches t s q = false := by
  rw [resolve] at h
  split at h
  · exact absurd h (by simp)
  · next hn =>
    refine ⟨by simpa using hn, ?_⟩
    split at h
    · next r hfc =>
      obtain ⟨hmatch, pre, post, heq, hpre⟩ := firstClaim_inv hfc
      have hc : r.claimant = c := by injection h
      have ha : r.action = a := by injection h
      exact ⟨pre, r, post, heq, hc, ha, hmatch, hpre⟩
    · next => split at h <;> exact absurd h (by simp)

/-- Inserting a declining rung anywhere in a list leaves the scan's answer
    alone. The induction behind `declining_rung_is_transparent`. -/
theorem firstClaim_skip_declining [BEq T] [BEq C] (pre post : List (Rung T C))
    (t : T) (s : Situation C) {r : Rung T C} (h : rungMatches t s r = false) :
    firstClaim (pre ++ r :: post) t s = firstClaim (pre ++ post) t s := by
  induction pre with
  | nil => simp [firstClaim, h]
  | cons q rest ih =>
    simp only [List.cons_append, firstClaim, ih]

/-- **The registration-safety theorem — the real content of this file.**

    A rung that does not own the trigger, or whose claimant is not live, is
    TRANSPARENT: inserting it at ANY index leaves the verdict unchanged, for
    every ladder, trigger and situation. (`pre ++ post` ranges over every split
    of the existing table, so "any index" is not an approximation.)

    STATUS: `proven`. This is what licenses a new claimant registering a rung
    instead of standing up its own input loop: a rung that declines cannot
    perturb anybody else's verdict, so registration is safe by construction
    rather than by review. It does NOT establish that a registered claimant
    *declines when it should* — that is the consumer's claim accessor, and the
    consumer-side conformance test is what checks it.

    Note what is deliberately not assumed: no well-formedness hypothesis on the
    ladder, no uniqueness of claimant names, no disjointness of triggers. The
    theorem holds over hostile tables, which is the same posture as
    `Ladder::new` being infallible. -/
theorem declining_rung_is_transparent [BEq T] [BEq C]
    (L : Ladder T C) (t : T) (s : Situation C) {r : Rung T C}
    (pre post : List (Rung T C)) (hL : L.rungs = pre ++ post)
    (hdecline : rungMatches t s r = false) :
    resolve { L with rungs := pre ++ r :: post } t s = resolve L t s := by
  simp only [resolve, hL, firstClaim_skip_declining pre post t s hdecline]

/-! ## Instrumentation for the generated vector block

    `Precedence.Vectors` is emitted by `just gen-vectors` from the Rust truth
    table. It is pure data plus three `decide` theorems; the helpers those
    theorems are stated in live here, so the generator emits no logic. -/

/-- One golden vector: an input and the verdict the Rust `resolve` produced. -/
structure Case (T C : Type) where
  /-- The trigger pressed. -/
  trigger : T
  /-- Which claimants were live. -/
  claiming : List C
  /-- Was work running? -/
  workRunning : Bool
  /-- What Rust's `resolve` returned. -/
  expected : Verdict C
  deriving Repr

/-- The situation a case describes. -/
def Case.situation (c : Case T C) : Situation C := ⟨c.claiming, c.workRunning⟩

/-- Does this model agree with the recorded Rust verdict? -/
def caseOk [BEq T] [BEq C] (L : Ladder T C) (c : Case T C) : Bool :=
  resolve L c.trigger c.situation == c.expected

/-- What `L` answers across a whole case list, ignoring what was recorded. Two
    ladders with the same outcome vector are indistinguishable on that grid. -/
def outcomes [BEq T] [BEq C] (L : Ladder T C) (cs : List (Case T C)) : List (Verdict C) :=
  cs.map (fun c => resolve L c.trigger c.situation)

/-- `(claimed, escape, unbound)` counts over a case list — the distribution the
    Rust truth table asserts by hand. A grid that produced only one of the
    three would satisfy `caseOk` for free; this is what rules that out. -/
def tally [BEq T] [BEq C] (L : Ladder T C) (cs : List (Case T C)) : Nat × Nat × Nat :=
  (outcomes L cs).foldl
    (fun (acc : Nat × Nat × Nat) v =>
      match v with
      | .claimed _ _ => (acc.1 + 1, acc.2.1, acc.2.2)
      | .escape _ => (acc.1, acc.2.1 + 1, acc.2.2)
      | .unbound => (acc.1, acc.2.1, acc.2.2 + 1))
    (0, 0, 0)

end Precedence
