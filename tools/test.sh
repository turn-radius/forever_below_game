#!/usr/bin/env bash
# Runs the workspace's tests with one process per test binary, every binary at once.
#
#   tools/test.sh                     every test the default run includes
#   tools/test.sh rarity              only tests whose name contains "rarity"
#   tools/test.sh --include-ignored   the slow sweeps too
#
# Arguments go to every test binary as they would after `cargo test --workspace --`.
#
# Why not nextest: it starts a process per test, and on a machine whose endpoint security
# vets every launch in one queue (about 0.27 s each here), 1,700 tests wait minutes to start.
# This starts each binary once and lets it run its tests on threads, as `cargo test` does,
# but runs the binaries side by side instead of one after another.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
out="$(mktemp -d "${TMPDIR:-/tmp}/fb-test.XXXXXX")"
trap 'rm -rf "$out"' EXIT

started=$(date +%s)
# Build every test target once; each line names an executable and the package it belongs to.
if ! cargo test --workspace --no-run --message-format=json 2>"$out/build.log" \
    | jq -r 'select(.reason == "compiler-artifact" and .profile.test == true and .executable != null)
             | "\(.executable)\t\(.manifest_path)\t\(.target.name)\t\(.target.kind[0])"' >"$out/bins.tsv"; then
  cat "$out/build.log" >&2
  echo "tools/test.sh: the tests did not build" >&2
  exit 1
fi
built=$(date +%s)

# Each binary runs from its package's folder with the variables cargo gives a test.
i=0
while IFS=$'\t' read -r exe manifest name kind; do
  dir="$(dirname "$manifest")"
  package="$(basename "$dir")"
  label="$package $kind $name"
  [ "$kind" = "lib" ] && label="$package"
  i=$((i + 1))
  echo "$label" >"$out/$i.label"
  (
    cd "$dir"
    CARGO_MANIFEST_DIR="$dir" CARGO_PKG_NAME="$package" "$exe" "$@" >"$out/$i.log" 2>&1
    echo $? >"$out/$i.code"
  ) &
done <"$out/bins.tsv"
wait
finished=$(date +%s)

# One line per binary, sorted; then each failure in full.
for code in "$out"/*.code; do
  n="$(basename "$code" .code)"
  label="$(cat "$out/$n.label")"
  if [ "$(cat "$code")" != 0 ]; then
    printf '%-40s FAILED\n' "$label"
  else
    result="$(grep -E '^test result:' "$out/$n.log" | tail -1)"
    printf '%-40s %s\n' "$label" "$(echo "$result" | sed -E 's/.*ok\. ([0-9]+ passed).*([0-9]+ ignored).*finished in ([0-9.]+s).*/\1, \2, \3/')"
  fi
done | sort
for code in "$out"/*.code; do
  n="$(basename "$code" .code)"
  [ "$(cat "$code")" = 0 ] && continue
  echo
  echo "=== $(cat "$out/$n.label")"
  # The failures section libtest prints, else the whole log (a crash, say).
  if grep -q '^failures:$' "$out/$n.log"; then
    sed -n '/^failures:$/,$p' "$out/$n.log"
  else
    cat "$out/$n.log"
  fi
done

total="$(cat "$out"/*.log | grep -E '^test result:' | sed -E 's/.* ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored.*/\1 \2 \3/' \
  | awk '{p += $1; f += $2; g += $3} END {printf "%d passed, %d failed, %d ignored", p, f, g}')"
bad="$(grep -L '^0$' "$out"/*.code | wc -l | tr -d ' ')"
took=$((finished - built))
echo
echo "$total across $i binaries; build $((built - started)) s, tests $took s"
# The slowest binaries bound the run, so name them, and say when the run is over budget: a new
# slow test is then noticed in the change that adds it. Put a slow sweep behind #[ignore] with a
# quick _smoke twin.
for n in $(seq 1 "$i"); do
  secs="$(grep -E '^test result:' "$out/$n.log" | tail -1 | sed -nE 's/.*finished in ([0-9.]+)s.*/\1/p')"
  [ -n "$secs" ] && echo "$secs $(cat "$out/$n.label")"
done | sort -rn | head -5 | awk '{t = $1; $1 = ""; printf "  slowest:%s, %s s\n", $0, t}'
budget="${FB_TEST_BUDGET:-120}"
if [ "$took" -gt "$budget" ]; then
  echo "over budget: the tests took $took s against $budget s"
fi
[ "$bad" = 0 ]
