# justfile for precedence-ladder.
#
# `just check` runs the full local gate (fmt + clippy + test + doc + leaf), the
# same steps enforced by .githooks/pre-push and .github/workflows/ci.yml. The
# CI-only `msrv` (1.88) job is intentionally not in `check` — see the pre-push
# hook header for the rationale. Run it with `just msrv`.

# Run the full local check suite: format, lint, test, doc, leaf guard.
check: fmt clippy test doc leaf

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
