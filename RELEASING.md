# Releasing `precedence-ladder`

Releases are **tag-driven**. Pushing a `v*` tag runs
[`.github/workflows/release.yml`](.github/workflows/release.yml), which — only
after one common gate passes — publishes the crate to **crates.io** and creates
the **published** GitHub Release whose body is this file's sibling
`CHANGELOG.md` section, verbatim.

> **Current line: `0.1.0-rc.1`.** The first release candidate. It goes to
> crates.io marked as a prerelease so the packaging, the MSRV floor and the
> `cid` byte encoding are exercised against a real registry before `0.1.0`
> freezes them. The PyPI face is still slice C3; nothing publishes there yet.

## The gate (what must be green before anything publishes)

Nothing publishes until `release-gate`, and `release-gate` needs the whole DAG —
so a route to publishing past a red check does not exist:

```
provenance ─┐
rust-gate ──┼─► release-gate ─► publish-crate ─► github-release
msrv ───────┤                   (crates.io)     (the published Release)
formal ─────┘
```

**The Release comes after the crate, not beside it.** `publish-crate` sits
behind the protected `crates-io` environment — behind a human approval that can
be rejected or time out. Running the Release in parallel with that approval
makes the two failure modes asymmetric: a Release published before a rejected
publish announces a version nobody can `cargo add`, and no re-run takes that
back. Serialized, the worst case is a tag and a crate with the Release still to
come — an unfinished release, but the recoverable kind: re-run one idempotent
job. The Release lands a minute later and never describes a crate that is not
there.

| Job | What it proves |
|---|---|
| `provenance` | The version declarations agree (`Cargo.toml` **and** `Cargo.lock`, via `scripts/verify_release.py`), the tag is exactly `v<Cargo.toml version>`, the tag **is** the checked-out `HEAD`, that commit is contained in `main` (`git merge-base --is-ancestor`), and `CHANGELOG.md` has a section for this version. A tag on an out-of-band commit is rejected before anything is built. |
| `rust-gate` | `just check` (fmt + clippy + test + doc + leaf + vectors + `sorry` gate, in all three feature settings) + `just verify-release` + `cargo test --locked` ×3 + **`cargo publish --dry-run --locked`** + the packaged-tarball check below. |
| `msrv` | Build + test on the pinned MSRV `1.88`, so the declared floor holds at the commit being released. |
| `formal` | `lake build` — every Lean theorem, including the generated `decide` block re-deriving all **704** golden verdicts — plus the `sorry` grep. |
| `release-gate` | Aggregator. The two publishes depend only on this. |

`formal` is **duplicated** from `.github/workflows/formal.yml` on purpose: that
workflow is paths-filtered and triggers on branch pushes, not tags, so a tag
would otherwise inherit a green that was never computed for it. A release that
skips the proofs is not this line's release. The two jobs are a mirrored pair —
edit one, edit the other.

The same per-push gate also runs locally: `just check`, and
`.githooks/pre-push`, which mirrors it plus the release-notes check.

## What ships in the tarball

`Cargo.toml` declares no `include`/`exclude`, so `cargo package` ships every
git-tracked file. That is **deliberate, and asserted**: `rust-gate` fails if
`formal/`, `spec/vectors/`, `examples/gen_vectors.rs` or the two check scripts
fall out of `cargo package --list`.

The reason is that this crate's claims are checkable claims. A consumer who
downloads the tarball can run `lake build` in `formal/` to re-check the
theorems, and `./scripts/check-vectors.sh` to re-derive all 704 verdicts from
the shipped `resolve` — without cloning the repo or trusting a badge. Shipping
the proofs is a few hundred kilobytes; shipping a crate whose evidence lives
somewhere else is a different kind of artifact.

If that trade ever stops being worth it, add the `exclude` **and** update the
job's list, so the change is a visible decision rather than a silent one.

## Version strings — up to four, in two spellings

| Declaration | File | Spelling | Today |
|---|---|---|---|
| Rust core crate | `Cargo.toml` `[package].version` | SemVer | `0.1.0-rc.1` |
| The committed lockfile | `Cargo.lock`, this crate's own `[[package]]` | SemVer, identical | `0.1.0-rc.1` |
| PyO3 binding crate (C3) | `precedence-ladder-py/Cargo.toml` | SemVer, identical | absent |
| Python distribution (C3) | `pyproject.toml` `[project].version` | PEP 440 | absent |

`scripts/verify_release.py` is the **single** implementation of the SemVer ↔
PEP 440 mapping — no second copy in YAML or shell. It checks the declarations
that exist and skips the ones that do not, so C3 adds its two manifests without
editing the script; `--require-all` turns their absence into an error once they
are there.

`Cargo.lock` is checked because this repo commits its lockfile and publishes
`--locked`. A bump that edits `Cargo.toml` and forgets `cargo check` used to
fail at `cargo publish --locked` — the last, irreversible step — instead of at
the first.

### SemVer → PEP 440 (the only supported forms)

```
0.1.0          -> 0.1.0
0.1.0-alpha.1  -> 0.1.0a1
0.1.0-beta.2   -> 0.1.0b2
0.1.0-rc.3     -> 0.1.0rc3
```

**The prerelease number needs a dot in SemVer** (`-rc.1`, not `-rc1`) and none
in PEP 440 (`rc1`). Everything else fails closed: build metadata (`+…`),
`-alpha` with no number, `-pre`, `-dev`, leading zeros. The tag is always `v` +
the exact `Cargo.toml` SemVer string (`v0.1.0-rc.1`).
`python3 scripts/verify_release.py --self-test` asserts all of it — plus an
anti-vacuous twin proving the lockfile check really fires — and the
`version-drift` CI job runs it on every PR, so the mapping cannot rot between
releases and surprise a publish.

## Cutting a release

1. **Bump every version declaration** on its own branch/PR — do not fold a
   version bump into unrelated work. Edit `Cargo.toml`, then run `cargo check`
   so `Cargo.lock` follows.

2. **Write the `CHANGELOG.md` entry.** The release notes are the CHANGELOG
   section **verbatim**, so the entry is the release body — write it as the
   thing a reader will see, not as a note-to-self. Leave a fresh `[Unreleased]`
   above it, and add the `[x.y.z]:` and `[#N]:` link definitions at the foot;
   `scripts/release-notes.sh` carries them into the release body, and without
   them every `[#N]` renders as literal text.

3. **Rehearse locally.** Publishes nothing:

   ```sh
   just check                        # the per-push gate
   just release-dryrun v0.1.0-rc.1   # versions + notes + package list + publish dry run
   just release-notes                # read the actual release body
   just msrv                         # the pinned floor
   just lean                         # every Lean theorem (needs a Lean toolchain)
   ```

   `release-dryrun` needs a **clean working tree**: `cargo package` refuses to
   pack uncommitted changes, and rightly so — the tarball is built from what git
   has, so a rehearsal against a dirty tree would rehearse the wrong bytes.
   Commit first; do not reach for `--allow-dirty`.

   A dry run of the whole CI gate (publishing nothing, creating no Release) is
   also available from **Actions → release → Run workflow**.

4. **Merge the PR**, and confirm CI is green on `main`.

5. **GPG-signed annotated tag** on the merge commit — never a branch tip. An
   agent pane has no pinentry TTY, so unlock the key in a focused terminal and
   sign within the cache TTL; see the `gpg-signed-tag` skill. **Never
   `--no-sign`.** The signature is the provenance the workflow does not verify
   and cannot reconstruct.

   ```sh
   export GPG_TTY=$(tty)
   echo test | gpg --local-user 5935B06EF479624C --detach-sign -o /dev/null - && echo cached
   git checkout main && git pull --ff-only
   git tag -a v0.1.0-rc.1 -m "precedence-ladder v0.1.0-rc.1"
   git tag -v v0.1.0-rc.1            # must print "Good signature"
   ```

6. **Push the tag.** This is the irreversible step:

   ```sh
   git push origin v0.1.0-rc.1
   ```

7. **The workflow does the rest** — the gate, then `cargo publish`, then the
   GitHub Release. The `crates-io` environment holds a required reviewer, so
   the publish waits for an explicit human approval in the Actions run.

8. **Verify from outside**, not from the workflow's green:

   ```sh
   # Ask for the EXACT version, not `max_version`: crates.io reports
   # `max_stable_version` separately, and a prerelease is exactly the case
   # where the two disagree. 200 means published; 404 means it is not there.
   curl -so /dev/null -w '%{http_code}\n' \
     https://crates.io/api/v1/crates/precedence-ladder/0.1.0-rc.1
   gh release view v0.1.0-rc.1 --json name,isDraft,isPrerelease,url
   git tag -v v0.1.0-rc.1     # "Good signature" — from the remote's copy after a fresh clone
   ```

   Done means all three: the registry has the version, the Release renders with
   `isDraft=false`, and the tag verifies. A pushed tag with no *published*
   Release is an unfinished release.

**The GitHub Release is PUBLISHED, never drafted** — `--prerelease` for an rc,
which the workflow derives from the tag containing a SemVer prerelease hyphen
rather than from a hand-set flag. A pushed tag with no *published* Release is an
unfinished release. Do not inherit newt's `draft: true` +
`generate_release_notes: true`, which makes every tag an unfinished release by
construction.

Two hazards worth stating before they bite: publishing a draft whose tag is
missing **creates** that tag, and GitHub ranks *Latest* by publish time — so
publishing an old draft hijacks it until you re-pin with `--latest`.
Prereleases never take *Latest*, so an rc is safe in that second respect.

### If a publish partially fails

**Re-run only the failed job** from the Actions tab. `github-release` is
idempotent — it edits an existing Release rather than failing on it, and forces
`--draft=false` on that path — and `cargo publish --locked` resolves the same
lockfile the gate already validated. A partial failure is a one-job re-run,
**never** a re-tag.

Because the Release is serialized behind the crate, the only partial state
reachable is *crate published, Release missing*, which is exactly the one a
re-run fixes. Do not "re-run all jobs" to get there: `publish-crate` will go red
on the already-published version. Re-run `github-release` alone.

## crates.io authentication: token now, OIDC later

The workflow authenticates with a `CARGO_REGISTRY_TOKEN` secret, and that is
**forced, not preferred**. crates.io Trusted Publishing has no
pending-publisher flow: configuring it requires already being an owner of an
**existing** crate, and `precedence-ladder` does not exist on crates.io yet.
The first publish is therefore necessarily token-authenticated.

So: `0.1.0-rc.1` creates the crate with a scoped token. Then register trusted
publishing, replace the `publish-crate` step, and **delete the secret** before
`0.1.0`. That order is the only one available.

Do **not** publish by hand from a laptop with `~/.cargo/credentials.toml`. That
token is an interactive-machine credential with no gate, no provenance, and no
gate-before-publish; the tag-driven workflow exists to replace it. Its presence
on the dev box is not a reason to use it.

## One-time maintainer setup

CI cannot see or assert any of these. Confirm before the first `v*` tag:

- [ ] **Owner decided.** The repo lives under `Gilamonster-Foundation`. The
      *PyPI* trusted-publisher registration (C3) is owner-pinned and cannot
      drift afterwards; crates.io ownership is transferable, so only the PyPI
      half is irreversible.
- [ ] **Create a scoped crates.io token** (<https://crates.io/settings/tokens>,
      scope *publish-new* + *publish-update*, ideally name-scoped to
      `precedence-ladder`) and add it as the repository secret
      **`CARGO_REGISTRY_TOKEN`**, scoped to the `crates-io` environment.
- [ ] **Create the `crates-io` deployment environment** (Settings →
      Environments) with a required reviewer, so a human gate sits in front of
      the irreversible publish.
- [ ] **Confirm Actions can write the Release object.** Settings → Actions →
      General → *Workflow permissions* currently reads **"Read repository
      contents and packages permissions"** on this repo. `github-release` asks
      for `contents: write` explicitly, which normally overrides that default —
      but this is the one prerequisite whose failure necessarily lands *after*
      the crate is already public, since `github-release` runs after
      `publish-crate` by design. Verify it (or flip the setting to *Read and
      write*) before the first tag rather than discovering it from a tag with no
      Release. If it does bite: fix the setting and re-run the single
      `github-release` job — it is idempotent, and `--verify-tag` means it can
      never mint a tag. **Never re-tag.**
- [ ] **Protect `main`** (require the PR + CI checks; no direct pushes) and
      **protect the `v*` tags** so only maintainers can create release tags.
      The `provenance` job asserts the released commit is contained in `main`,
      but that guarantee is only as strong as branch protection.
- [ ] **After the first publish:** register crates.io Trusted Publishing for
      this crate (workflow `release.yml`, environment `crates-io`), switch
      `publish-crate` to it, and delete `CARGO_REGISTRY_TOKEN`.

Still C3, not needed for this line: the PyPI trusted publisher, the `pypi`
environment, and the `precedence-ladder-py` / `pyproject.toml` declarations.
