# Releasing `precedence-ladder`

> **Status: not releasable yet.** Slice C1 (this repo's current state) ships
> the crate, the guards, and CI. The release workflow, the PyO3 wheel, and the
> publish gate land in **slice C3**. Nothing has been published to crates.io or
> PyPI, and the trusted-publisher registration has deliberately not been made —
> it is owner-pinned and cannot drift afterwards, so it waits on the operator's
> answer to §8 Q1 of the build plan.
>
> This file records the shape the release will take, and the parts of it that
> already exist and already run. Everything under "Cutting a release" is
> **C3's** work; do not attempt it before that workflow lands.

## The gate (what must be green before anything publishes)

Today, and already enforced on every PR by `.github/workflows/ci.yml` and
locally by `just check` / `.githooks/pre-push`:

| Check | Where |
|---|---|
| `cargo fmt -- --check` | `fmt` job, hook |
| `cargo clippy --all-targets -- -D warnings`, all-features **and** no-default-features | `clippy` job, hook |
| `cargo test` in all three feature configurations | `test` job, hook |
| `cargo doc` with `RUSTDOCFLAGS=-D warnings` | `doc` job, hook |
| Leaf invariant (registry-only closure, `--all-features`) | `leaf-deps` job, hook, `just leaf` |
| Version declarations agree + PEP 440 mapping self-test | `version-drift` job, `just verify-release` |
| The dependency guard (empty default closure, no ambient authority) | inside `cargo test` — `tests/guard.rs` |

Landing in **C2**: `lake build` over `formal/`, plus a `sorry` grep. `lake
build` exits 0 on a `sorry`, so "sorry-free" without that grep is a human
assertion, not a gate.

Landing in **C3**: the MSRV job already exists; the release workflow adds
`provenance` → `{rust-gate, msrv, formal}` → `{build-wheels, build-sdist}` →
`{install-smoke, sdist-smoke}` → `release-gate` → `{publish-pypi,
publish-crate}`.

## Version strings — there are up to three, in two spellings

| Declaration | File | Spelling |
|---|---|---|
| Rust core crate | `Cargo.toml` `[package].version` | SemVer |
| PyO3 binding crate (C3) | `precedence-ladder-py/Cargo.toml` | SemVer, identical |
| Python distribution (C3) | `pyproject.toml` `[project].version` | PEP 440 |

`scripts/verify_release.py` is the **single** implementation of the SemVer ↔
PEP 440 mapping — there is no second copy in YAML or shell. It checks the
declarations that exist and skips the ones that do not, so C3 adds its two
manifests without editing the script; `--require-all` turns their absence into
an error once they are there.

### SemVer → PEP 440 (the only supported forms)

```
0.1.0          -> 0.1.0
0.1.0-alpha.1  -> 0.1.0a1
0.1.0-beta.2   -> 0.1.0b2
0.1.0-rc.3     -> 0.1.0rc3
```

Everything else fails closed: build metadata (`+…`), `-alpha` with no number,
`-pre`, `-dev`, leading zeros. `python3 scripts/verify_release.py --self-test`
asserts all of it, and the `version-drift` CI job runs that self-test on every
PR, so the mapping cannot rot between releases and surprise a publish.

## Cutting a release (C3 — not yet possible)

1. Update `CHANGELOG.md` — the release notes are the CHANGELOG **verbatim**.
2. Bump every version declaration; `just verify-release v<X.Y.Z>` must pass.
3. `just check` and `just msrv` green.
4. **GPG-signed annotated tag** via the `gpg-signed-tag` skill — an agent pane
   has no pinentry TTY, so unlock the key in a focused sibling pane and sign
   within the cache TTL. **Never `--no-sign`.**
5. Push the tag; the release workflow runs the gate.
6. **The GitHub Release is PUBLISHED, never drafted** (`prerelease: true` for an
   rc). A pushed tag with no *published* Release is an unfinished release.
   Do not inherit newt's `draft: true` + `generate_release_notes: true`, which
   makes every tag an unfinished release by construction.

Two hazards worth stating before they bite: publishing a draft whose tag is
missing **creates** that tag, and GitHub ranks *Latest* by publish time — so
publishing an old draft hijacks it until you re-pin with `--latest`.

### If a publish partially fails

Publishes never rebuild: both publish jobs `needs: [release-gate]` and
download exactly the artifacts the gate validated. A partial failure is a
one-job re-run, never a re-tag.

## One-time maintainer setup (C3)

Not done, on purpose — each item is a human decision, and the first is
irreversible:

1. **Decide the owner** (`Gilamonster-Foundation` vs `hartsock`). The PyPI
   trusted-publisher registration is owner-pinned. The repo lives under
   `Gilamonster-Foundation` today.
2. **Register the PyPI Trusted Publisher (OIDC, `id-token: write`)** — not an
   account-scoped API token.
3. **Add the crates.io token** to a protected `crates-io` environment.
4. **Create the `pypi` and `crates-io` deployment environments** with required
   reviewers.
5. **Protect `main` and the `v*` tags.**
