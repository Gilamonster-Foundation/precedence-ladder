# justfile for precedence-ladder.
#
# `just check` runs the full local gate (fmt + clippy + test + doc + leaf +
# vectors + no-sorry), the same steps enforced by .githooks/pre-push and
# .github/workflows/{ci,formal}.yml. Two CI jobs are intentionally not in
# `check`, both documented in the pre-push hook header: `msrv` (1.88, a second
# toolchain) and `formal`'s `lake build` (a Lean toolchain). Run them with
# `just msrv` and `just lean`.
#
# The RELEASE gate (.github/workflows/release.yml) is a superset: its
# `rust-gate` job runs `just check` verbatim and adds `--locked`, the publish
# dry run and a packaged-tarball check. Rehearse all of that locally with
# `just release-dryrun` before asking for a signed tag — it publishes nothing.

# Run the full local check suite: format, lint, test, doc, leaf guard, vectors.
#
# `lean` is deliberately NOT here — see the `lean` recipe's comment and the
# pre-push hook header. `vectors` and `no-sorry` are, because both are cheap and
# both gate real drift.
check: fmt clippy test doc leaf vectors no-sorry

# Verify formatting (does not modify files).
fmt:
    cargo fmt -- --check

# Lint with all warnings denied, at every feature setting that ships.
# `--all-features` compiles the default-OFF `cid` feature so its lints are
# checked; `--no-default-features` compiles the bare predicate, which is the
# configuration a wasm or embedded consumer actually gets.
clippy:
    cargo clippy --all-targets --all-features -- -D warnings
    cargo clippy --all-targets --no-default-features -- -D warnings

# Run all tests (unit + integration + doctests) in the three configurations
# that ship. Plain `cargo test` includes doctests, which `--all-targets` skips.
#
#   --no-default-features  the bare predicate — this is the configuration
#                          `tests/guard.rs` proves has an EMPTY dependency
#                          closure, so it has to actually compile and pass.
#   (default)              + `table`: the ladder as data.
#   --all-features         + `cid`: content identity via content-addressable.
test:
    cargo test --no-default-features
    cargo test
    cargo test --all-features

# Build the docs with broken intra-doc links denied (mirrors the CI `doc` job).
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Assert this crate is a true leaf — its resolved dependency closure must be
# registry-only (no path/git/workspace edges) at every feature setting, so any
# repo can depend on it without cycles. Mirrors the CI `leaf-deps` job and the
# pre-push hook; reads only `cargo metadata`, so it is fast.
leaf:
    ./scripts/check-leaf-deps.sh

# Regenerate the golden vectors from the Rust truth table.
#
# Writes `spec/vectors/ladder.json` (data, for the C3 Python consumer) and
# `formal/Precedence/Vectors.lean` (a `decide` block, because a Mathlib-free
# Lean has no JSON reader). Both are GENERATED — never hand-edit them; run this
# and commit the result.
gen-vectors:
    cargo run --quiet --example gen_vectors --features cid,table -- .

# Assert the checked-in vectors match a fresh run. Mirrors the CI `vectors` job.
#
# This is the ONLY thing tying the Lean proofs to the shipped Rust: the theorems
# in `formal/Precedence/Basic.lean` are about the Lean model, and nothing in
# Lean reads `src/lib.rs`.
vectors:
    ./scripts/check-vectors.sh

# The `sorry` gate. Mirrors half of the CI `formal` job.
#
# `lake build` exits 0 on a `sorry`, so without this "sorry-free" would be a
# human assertion CI never checks. Pure grep — no Lean toolchain needed, which
# is why this half of `formal.yml` IS mirrored in the push hook and `lake build`
# is not.
no-sorry:
    ./scripts/check-lean-proofs.sh

# Check every Lean theorem. Mirrors the CI `formal` job's `lake build` step.
#
# NOT in `just check` and NOT in the push hook: it needs a Lean toolchain (via
# elan, version pinned in `formal/lean-toolchain`), and requiring a full Lean
# install of everyone who pushes would be disproportionate for a Rust crate.
# CI-only by design — the documented exception, recorded in both the workflow
# and the hook header.
lean:
    cd formal && lake build

# Verify the package version declarations agree via the canonical tool, and
# self-test the SemVer<->PEP 440 mapping. This is the SINGLE source of that
# mapping (no second copy in YAML or shell); the C3 release gate calls this
# same recipe. Kept out of `check` so a push never requires Python.
#   just verify-release            # drift guard: the declarations agree
#   just verify-release v0.1.0     # release: also assert the tag matches
verify-release tag="":
    #!/usr/bin/env bash
    set -euo pipefail
    python3 scripts/verify_release.py --self-test
    if [ -n "{{ tag }}" ]; then
      python3 scripts/verify_release.py --tag "{{ tag }}"
    else
      python3 scripts/verify_release.py
    fi

# Print a version's release notes: its CHANGELOG section VERBATIM plus the
# reference-link definitions from the foot of the file. This is exactly what
# `.github/workflows/release.yml` feeds to `gh release create --notes-file`, so
# running it is how you read the release body before the tag exists.
#   just release-notes             # the version declared in Cargo.toml
#   just release-notes v0.1.0-rc.1 # a specific one (leading `v` optional)
release-notes version="":
    ./scripts/release-notes.sh {{ version }}

# The local release rehearsal. Everything the release gate does that does NOT
# need a tag, a second toolchain, or the network to accept anything — run it
# before asking the operator for a signed tag.
#
# It publishes NOTHING. `cargo publish --dry-run` packs the crate and builds it
# from the packed tarball, which is the step that catches missing metadata and
# files that only exist in the working tree.
#
#   just release-dryrun                # rehearse against Cargo.toml's version
#   just release-dryrun v0.1.0-rc.1    # also assert the intended tag matches
#
# NOT in `just check` and NOT in the push hook: it needs Python and it rebuilds
# the crate from a fresh tarball. The push hook mirrors the one piece of this
# that is pure text — that the declared version has a CHANGELOG section.
release-dryrun tag="":
    #!/usr/bin/env bash
    set -euo pipefail
    just verify-release "{{ tag }}"
    echo
    echo "==> ./scripts/release-notes.sh (the release body)"
    ./scripts/release-notes.sh "{{ tag }}" | head -3
    echo "    ... $(./scripts/release-notes.sh "{{ tag }}" | wc -l) lines total"
    echo
    echo "==> cargo package --list --locked"
    cargo package --list --locked
    echo
    echo "==> cargo publish --locked --dry-run"
    cargo publish --locked --dry-run
    echo
    echo "release-dryrun OK — nothing was published."

# Build + test on the pinned MSRV (1.88). Mirrors the CI-only `msrv` job; run
# it manually, since installing a second toolchain is too heavy for a push hook.
# The floor comes from the OPTIONAL `cid` graph, not from our code — see the
# Cargo.toml comment.
msrv:
    rustup toolchain install 1.88 --profile minimal
    cargo +1.88 build --all-targets --all-features
    cargo +1.88 test --all-features
    cargo +1.88 test --no-default-features

# Apply rustfmt in place.
fmt-fix:
    cargo fmt

# Install the repo-local git hooks (pre-push).
install-hooks:
    git config core.hooksPath .githooks
    @echo "git hooks installed (core.hooksPath = .githooks)"
