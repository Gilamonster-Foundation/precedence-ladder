#!/usr/bin/env bash
# Extract one version's CHANGELOG section, VERBATIM, plus the reference-link
# definitions from the foot of the file.
#
# The release body IS the CHANGELOG section — not generated notes, not a
# summary. `.github/workflows/release.yml` pipes this into
# `gh release create --notes-file`, `just release-notes` prints it for a human
# to read before tagging, and `.githooks/pre-push` runs it as a drift check.
#
# WHY THE LINK DEFINITIONS COME TOO. Markdown reference links are resolved
# per-document. A body containing `[#3]` renders as the literal text `[#3]` on
# the Release page unless the `[#3]: https://…` line travels with it. That is a
# silent cosmetic failure — nothing errors, the links just stop being links —
# so it is done mechanically here rather than remembered at release time.
#
# WHY IT IS ALSO A PUSH HOOK CHECK. The failure it catches is a version bump
# that forgets the CHANGELOG entry. Caught here, that is a red push seconds
# after the bump; caught at release time it is a tag already pushed with an
# empty release body. Pure text — no cargo, no Python, no network — which is
# why this half of the release gate IS mirrorable into the hook while
# `cargo publish --dry-run` is not.
#
# Usage: release-notes.sh [VERSION]     # VERSION defaults to Cargo.toml's
#        release-notes.sh 0.1.0-rc.1
#        release-notes.sh v0.1.0-rc.1   # a leading `v` is accepted and stripped
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
changelog="$root/CHANGELOG.md"

version="${1:-}"
if [ -z "$version" ]; then
  # The ONE place this file reads a version. `scripts/verify_release.py` is the
  # canonical tool and proves Cargo.toml, Cargo.lock and the tag agree; by the
  # time the release workflow runs this, that has already passed, so reading the
  # first `version = "…"` under [package] here is a convenience, not a second
  # source of truth.
  version="$(awk '/^\[package\]/{p=1;next} /^\[/{p=0} p && /^version = /{gsub(/^version = "|"$/,""); print; exit}' "$root/Cargo.toml")"
fi
version="${version#v}"

if [ -z "$version" ]; then
  echo "release-notes: could not determine a version (no argument, and no" >&2
  echo "[package] version in Cargo.toml)." >&2
  exit 1
fi

# The section runs from its own `## [VERSION]` heading to the next `## [`
# heading (exclusive), or to the link-definition block, whichever comes first.
section="$(awk -v want="## [$version]" '
  index($0, want) == 1 { inside = 1; print; next }
  inside && /^## \[/ { exit }
  inside && /^<!-- REFERENCE-LINK DEFINITIONS/ { exit }
  inside { print }
' "$changelog")"

# POSITIVE READ ASSERTION. An extractor that emits nothing must fail loudly:
# `gh release create --notes-file` accepts an empty file happily, so a silent
# miss here becomes a published release with a blank body — and the tag is
# already pushed by then. Ten lines is well under any real section and well
# above a heading that matched nothing but itself.
lines="$(grep -c '' <<<"$section" || true)"
if [ "$lines" -lt 10 ]; then
  echo "release-notes: CHANGELOG.md has no usable '## [$version]' section" >&2
  echo "(extracted $lines lines). Add the entry before tagging — the release" >&2
  echo "body is the CHANGELOG section verbatim, so an empty one ships empty." >&2
  echo >&2
  echo "Section headings currently present:" >&2
  grep -n '^## \[' "$changelog" >&2 || true
  exit 1
fi

# Every reference-link definition in the file, unfiltered. Copying a few unused
# ones is free; deciding which ones the section "needs" would mean parsing
# Markdown, and that parser's bugs would all fail open into dead links.
#
# Collected BEFORE anything is printed: under `set -o pipefail` a downstream
# `head` would SIGPIPE the grep and trip the failure branch below for a reason
# that has nothing to do with the CHANGELOG.
links="$(grep -E '^\[[^]]+\]: https?://' "$changelog" || true)"
if [ -z "$links" ]; then
  echo "release-notes: no reference-link definitions found in CHANGELOG.md;" >&2
  echo "any [#N] references in the body would render as literal text." >&2
  exit 1
fi

printf '%s\n\n%s\n' "$section" "$links"
