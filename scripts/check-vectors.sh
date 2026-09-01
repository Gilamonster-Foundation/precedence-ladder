#!/usr/bin/env bash
# The golden-vector drift check.
#
# `spec/vectors/ladder.json` and `formal/Precedence/Vectors.lean` are both
# GENERATED from `spec/vectors/newt_ladder.toml` by the real `Ladder::resolve`
# (`examples/gen_vectors.rs`). This regenerates them into a temp dir and diffs.
#
# WHY THIS EXISTS. The vectors are the only thing tying the Lean proofs to the
# shipped Rust: `Precedence.Basic`'s theorems are about the Lean model, and
# nothing in Lean reads `src/lib.rs`. Without this check, "one artifact, three
# consumers" would be one artifact plus copies somebody promised to keep in
# step — and the Lean copy is precisely the one that could go stale invisibly,
# because the proofs would keep passing about a table that no longer matches
# the code.
#
# So: change `resolve` and forget to regenerate, or hand-edit either generated
# file, and this goes red.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
generated=(spec/vectors/ladder.json formal/Precedence/Vectors.lean)

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cargo run --quiet --example gen_vectors --features cid,table \
  --manifest-path "$root/Cargo.toml" -- "$tmp"

status=0
for file in "${generated[@]}"; do
  # POSITIVE READ ASSERTION. An identical-files check passes trivially when
  # both sides are missing or empty, so require the FRESH output to be
  # substantial before believing an agreement.
  lines="$(wc -l <"$tmp/$file" 2>/dev/null || echo 0)"
  if [ "$lines" -lt 100 ]; then
    echo "check-vectors: freshly generated $file has only $lines lines." >&2
    echo "The generator is not producing the grid, so agreeing with the" >&2
    echo "checked-in copy would prove nothing." >&2
    exit 1
  fi
  if ! diff -u "$root/$file" "$tmp/$file"; then
    echo >&2
    echo "check-vectors: $file is stale. Run \`just gen-vectors\` and commit" >&2
    echo "the result — do not hand-edit a generated file." >&2
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "check-vectors: ${#generated[@]} generated artifacts match a fresh run."
fi
exit "$status"
