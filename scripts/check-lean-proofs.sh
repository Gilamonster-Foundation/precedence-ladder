#!/usr/bin/env bash
# The `sorry` gate — a REAL gate, not a badge.
#
# `lake build` exits 0 on a `sorry`: an unproven theorem is a warning, not an
# error, so "sorry-free" would otherwise be a human assertion that CI never
# checks. This is the check that makes it mechanical. (newt-agent's `formal/`
# has the identical gap; it is filed there, not fixed here.)
#
# `native_decide` is refused on the same grounds. It discharges a goal by
# running compiled code and adds `Lean.ofReduceBool` to the trusted base — the
# compiler and the runtime join the kernel as things you have to believe. Every
# `decide` in this tree is a kernel reduction, and `#print axioms` on the
# generated theorems shows `propext` at most.
#
# POSITIVE READ ASSERTION. An absence check fails OPEN: anything that shrinks
# the scanned text makes it MORE likely to pass, so a moved directory or a typo
# in the glob would report "clean" forever. This asserts it actually read the
# files first, and names the count it expects to have grown past.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
formal="$root/formal"

mapfile -t files < <(find "$formal" -name '*.lean' -not -path '*/.lake/*' | sort)
if [ "${#files[@]}" -lt 3 ]; then
  echo "check-lean-proofs: found only ${#files[@]} .lean files under $formal." >&2
  echo "The scan is not reading the formal tree, so the absence it reports" >&2
  echo "means nothing. Expected at least Precedence.lean, Precedence/Basic.lean" >&2
  echo "and the generated Precedence/Vectors.lean." >&2
  exit 1
fi

if grep -nE '\b(sorry|native_decide)\b' "${files[@]}"; then
  echo >&2
  echo "check-lean-proofs: an unproven or unkernel-checked declaration is above." >&2
  echo "\`lake build\` exits 0 on a \`sorry\`, so this is the only thing standing" >&2
  echo "between a stub and a green badge. Prove it or delete it." >&2
  echo >&2
  echo "This gate is deliberately DUMB: it does not parse Lean, so it also fires" >&2
  echo "on a MENTION inside a comment. That is the right trade — the alternative" >&2
  echo "is a comment-aware scanner whose bugs all fail open. If the hit is a" >&2
  echo "mention, reword the comment; do not weaken the needle." >&2
  exit 1
fi

echo "check-lean-proofs: ${#files[@]} Lean files, no sorry and no native_decide."
