#!/usr/bin/env python3
"""Canonical release-version verification for `precedence-ladder`.

The single source of truth for release version agreement. Both local
development (`just verify-release`) and CI (the release gate, slice C3) call
THIS tool — there is no second version-mapping implementation in YAML or shell.

It proves:

  1. Every Rust manifest present declares the *same* SemVer version.
  2. The Python project version, **when it exists**, is the canonical PEP 440
     equivalent of that Rust version.
  3. (When a tag is supplied) the release tag is exactly ``v<rust-version>``.

The declarations it reads:

  * ``Cargo.toml``                     -> ``[package].version``  (REQUIRED — the core crate)
  * ``Cargo.lock``                     -> this crate's own ``[[package]].version`` (REQUIRED)
  * ``precedence-ladder-py/Cargo.toml``-> ``[package].version``  (optional — the PyO3 binding, C3)
  * ``pyproject.toml``                 -> ``[project].version``  (optional — the Python distribution, C3)

``Cargo.lock`` is checked because this repo **commits its lockfile** (see
``.gitignore``) and the release publishes ``--locked``. A bump that edits
``Cargo.toml`` without refreshing the lock makes ``cargo publish --locked``
fail at the last, irreversible step of the release instead of here, at the
first — and a stale lock is exactly what a hand-edited version bump produces.
The crate name is read from ``Cargo.toml`` rather than hardcoded, so there is
one spelling of it, not two.

**The optional two are checked when the file exists and skipped when it does
not**, so slice C3 adds them without editing this script — and so a C1 run is
honest about only having one declaration to check rather than pretending to
compare three. `--require-all` makes their absence an error, which is what the
release gate uses once they exist.

Fail-closed: unparsable/duplicate-key TOML, non-string versions, version
disagreement, unsupported/ambiguous prerelease spellings, and build metadata
all exit non-zero with a diagnostic that names the offending file and value
(never a secret).

SemVer -> PEP 440 mapping (the only supported forms):

    0.1.0          -> 0.1.0
    0.1.0-alpha.1  -> 0.1.0a1
    0.1.0-beta.2   -> 0.1.0b2
    0.1.0-rc.3     -> 0.1.0rc3
"""

from __future__ import annotations

import argparse
import os
import sys
import re
import tempfile
import tomllib
from pathlib import Path

# (relative path, TOML key path to the version string, required?)
ROOT_CARGO = ("Cargo.toml", ("package", "version"), True)
PY_CARGO = ("precedence-ladder-py/Cargo.toml", ("package", "version"), False)
PYPROJECT = ("pyproject.toml", ("project", "version"), False)
CARGO_LOCK = "Cargo.lock"

# SemVer prerelease identifier -> PEP 440 prerelease letter.
_PRERELEASE = {"alpha": "a", "beta": "b", "rc": "rc"}

_CORE_RE = re.compile(r"\d+\.\d+\.\d+")
_PRE_RE = re.compile(r"(alpha|beta|rc)\.(\d+)")


class VerifyError(Exception):
    """A release-verification failure (always names the offending input)."""


def rust_to_pep440(semver: str) -> str:
    """Map a Rust SemVer version to its canonical PEP 440 spelling, or raise.

    Supported: ``X.Y.Z``, ``X.Y.Z-alpha.N``, ``X.Y.Z-beta.N``, ``X.Y.Z-rc.N``.
    Rejected (fail closed): build metadata (``+...``) and every other
    prerelease form (``-alpha`` without a number, ``-pre``, ``-dev``,
    ``-alpha.beta``, ...).
    """
    if "+" in semver:
        raise VerifyError(
            f"build metadata is not supported for a release version: {semver!r}"
        )
    if "-" not in semver:
        if not _CORE_RE.fullmatch(semver):
            raise VerifyError(f"not a valid X.Y.Z release version: {semver!r}")
        return semver
    core, pre = semver.split("-", 1)
    if not _CORE_RE.fullmatch(core):
        raise VerifyError(f"not a valid X.Y.Z core in version: {semver!r}")
    m = _PRE_RE.fullmatch(pre)
    if not m:
        raise VerifyError(
            f"unsupported prerelease {pre!r} in {semver!r}; "
            "expected alpha.N, beta.N, or rc.N"
        )
    kind, num = m.group(1), m.group(2)
    if num != str(int(num)):  # reject leading zeros ("01") — ambiguous
        raise VerifyError(f"non-canonical prerelease number in {semver!r}")
    return f"{core}{_PRERELEASE[kind]}{num}"


def _read_version(root: Path, rel: str, keys: tuple[str, ...]) -> str:
    path = root / rel
    if not path.is_file():
        raise VerifyError(f"missing version file: {rel}")
    try:
        # tomllib raises TOMLDecodeError on a duplicate key, so ambiguous /
        # duplicated version declarations fail closed here.
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        raise VerifyError(f"unparsable TOML in {rel}: {exc}") from exc
    node: object = data
    for key in keys:
        if not isinstance(node, dict) or key not in node:
            table = ".".join(keys[:-1])
            raise VerifyError(f"no [{table}] version declared in {rel}")
        node = node[key]
    if not isinstance(node, str):
        raise VerifyError(f"version in {rel} is not a string: {node!r}")
    return node


def _read_lock_version(root: Path, crate: str) -> str:
    """Return this crate's own version as recorded in the committed ``Cargo.lock``.

    Fails closed on a missing lockfile, a lockfile that does not mention this
    crate at all, and — the case that actually matters — a lockfile that
    mentions it more than once, which would make "the" locked version
    ambiguous.
    """
    path = root / CARGO_LOCK
    if not path.is_file():
        raise VerifyError(f"missing version file: {CARGO_LOCK}")
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        raise VerifyError(f"unparsable TOML in {CARGO_LOCK}: {exc}") from exc
    found = [
        pkg.get("version")
        for pkg in data.get("package", [])
        if isinstance(pkg, dict) and pkg.get("name") == crate
    ]
    if not found:
        raise VerifyError(f"{CARGO_LOCK} has no [[package]] entry for {crate!r}")
    if len(found) > 1:
        raise VerifyError(f"{CARGO_LOCK} has {len(found)} entries for {crate!r}: {found!r}")
    if not isinstance(found[0], str):
        raise VerifyError(f"version for {crate!r} in {CARGO_LOCK} is not a string: {found[0]!r}")
    return found[0]


def verify(root: Path, tag: str | None, require_all: bool = False) -> list[str]:
    """Return a list of problems; an empty list means the release is consistent."""
    problems: list[str] = []
    versions: dict[str, str] = {}
    for name, (rel, keys, required) in {
        "root-cargo": ROOT_CARGO,
        "py-cargo": PY_CARGO,
        "pyproject": PYPROJECT,
    }.items():
        if not (root / rel).is_file() and not (required or require_all):
            continue
        try:
            versions[name] = _read_version(root, rel, keys)
        except VerifyError as exc:
            problems.append(str(exc))
    if problems:
        return problems  # cannot compare if any declaration is unreadable

    rust = versions["root-cargo"]

    # 1. Every Rust manifest present must agree exactly.
    if "py-cargo" in versions and versions["py-cargo"] != rust:
        problems.append(
            f"Rust version disagreement: Cargo.toml={rust!r} != "
            f"{PY_CARGO[0]}={versions['py-cargo']!r}"
        )

    # 1b. The committed lockfile must record the same version. The release
    #     publishes `--locked`, so a stale lock is a failure at the last
    #     irreversible step unless it is caught here at the first.
    try:
        crate = _read_version(root, ROOT_CARGO[0], ("package", "name"))
        locked = _read_lock_version(root, crate)
        if locked != rust:
            problems.append(
                f"Cargo.lock records {crate} = {locked!r} but Cargo.toml declares "
                f"{rust!r} — run `cargo check` and commit the refreshed lockfile"
            )
    except VerifyError as exc:
        problems.append(str(exc))

    # 2. pyproject, when present, must be the canonical PEP 440 of the Rust
    #    version. The mapping is validated even when pyproject is absent, so a
    #    malformed Cargo version is caught in C1 rather than at first publish.
    expected_pep440: str | None = None
    try:
        expected_pep440 = rust_to_pep440(rust)
    except VerifyError as exc:
        problems.append(f"Cargo.toml version {rust!r}: {exc}")
    if (
        "pyproject" in versions
        and expected_pep440 is not None
        and versions["pyproject"] != expected_pep440
    ):
        problems.append(
            f"pyproject.toml version {versions['pyproject']!r} is not the PEP 440 "
            f"form of Cargo.toml {rust!r} (expected {expected_pep440!r})"
        )

    # 3. Optional tag must be exactly v<rust-version>.
    if tag is not None and tag != f"v{rust}":
        problems.append(
            f"release tag {tag!r} != expected {f'v{rust}'!r} "
            "(must be v<Cargo.toml version>)"
        )

    return problems


def self_test() -> int:
    """Assert the SemVer -> PEP 440 mapping, the one piece of real logic here.

    Run by `just verify-release`, so the mapping cannot rot silently between
    releases. No pytest, no fixtures — the check is the smallest thing that
    fails if the mapping breaks.
    """
    for semver, pep440 in [
        ("0.1.0", "0.1.0"),
        ("1.20.30", "1.20.30"),
        ("0.1.0-alpha.1", "0.1.0a1"),
        ("0.1.0-beta.2", "0.1.0b2"),
        ("0.1.0-rc.3", "0.1.0rc3"),
        ("0.8.0-rc.10", "0.8.0rc10"),
    ]:
        got = rust_to_pep440(semver)
        assert got == pep440, f"{semver!r} -> {got!r}, expected {pep440!r}"
    # Fail-closed cases. Each is a spelling that would publish a DIFFERENT
    # version to PyPI than to crates.io if it were silently accepted.
    for bad in [
        "0.1.0+build",
        "0.1.0-alpha",
        "0.1.0-pre.1",
        "0.1.0-dev",
        "0.1.0-alpha.01",
        "0.1",
        "v0.1.0",
        "0.1.0-alpha.beta",
    ]:
        try:
            mapped = rust_to_pep440(bad)
        except VerifyError:
            continue
        raise AssertionError(f"{bad!r} was accepted and mapped to {mapped!r}")

    # ANTI-VACUOUS TWIN for the lockfile check. A drift guard that never fires
    # is a green light over an unchecked tree, so prove it fires: the same
    # synthetic repo passes with an agreeing lock and fails with a stale one.
    # Written to a temp dir, so it cannot be satisfied by the real repo
    # happening to be consistent at the moment the self-test runs.
    manifest = '[package]\nname = "widget"\nversion = "9.9.9"\n'
    lock_tmpl = '[[package]]\nname = "widget"\nversion = "%s"\n'
    with tempfile.TemporaryDirectory() as tmp:
        fake = Path(tmp)
        (fake / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (fake / "Cargo.lock").write_text(lock_tmpl % "9.9.9", encoding="utf-8")
        agreeing = verify(fake, None)
        assert agreeing == [], f"agreeing lock reported problems: {agreeing!r}"
        (fake / "Cargo.lock").write_text(lock_tmpl % "9.9.8", encoding="utf-8")
        stale = verify(fake, None)
        assert any("Cargo.lock" in p for p in stale), f"stale lock went unreported: {stale!r}"
        (fake / "Cargo.lock").write_text('[[package]]\nname = "other"\nversion = "1.0.0"\n')
        absent = verify(fake, None)
        assert any("no [[package]]" in p for p in absent), f"absent crate unreported: {absent!r}"

    print("verify_release self-test OK (14 mapping cases + 3 lockfile cases)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Verify release version agreement across the package manifests."
    )
    parser.add_argument(
        "--tag",
        default=os.environ.get("RELEASE_TAG"),
        help="release tag to validate, e.g. v0.1.0 (or the RELEASE_TAG env var)",
    )
    parser.add_argument("--repo-root", default=Path("."), type=Path)
    parser.add_argument(
        "--require-all",
        action="store_true",
        help="treat a missing Python manifest as an error (the C3 release gate)",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="check the SemVer -> PEP 440 mapping and exit",
    )
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    tag = args.tag or None

    problems = verify(args.repo_root, tag, args.require_all)
    if problems:
        print("release version verification FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    rust = _read_version(args.repo_root, ROOT_CARGO[0], ROOT_CARGO[1])
    line = f"release version verification OK: Rust {rust!r} == PEP 440 {rust_to_pep440(rust)!r}"
    if tag is not None:
        line += f", tag {tag!r}"
    print(line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
