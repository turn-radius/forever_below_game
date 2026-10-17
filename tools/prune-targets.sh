#!/usr/bin/env bash
# Deletes the build caches (`target/`) of worktrees under .claude/worktrees/ that no longer
# need them. A worktree's own files and branch are left alone; only its `target/` goes.
#
#   tools/prune-targets.sh              worktrees whose HEAD is merged into origin/main
#   tools/prune-targets.sh --idle 24    also those whose build is untouched for 24 hours
#   tools/prune-targets.sh --dry-run    list what would go, delete nothing
#
# A worktree with cargo, rustc or the game running in it is never touched.
set -uo pipefail

root="$(git -C "$(dirname "$0")/.." rev-parse --path-format=absolute --git-common-dir)/.."
root="$(cd "$root" && pwd)"
worktrees="$root/.claude/worktrees"
dry=0
idle=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --idle) idle="$2"; shift ;;
    *) echo "usage: tools/prune-targets.sh [--dry-run] [--idle HOURS]" >&2; exit 2 ;;
  esac
  shift
done

git -C "$root" fetch -q origin main 2>/dev/null
# The working folders of every running build or game, to leave their worktrees alone.
busy="$(lsof -c cargo -c rustc -c forever_below -a -d cwd -Fn 2>/dev/null | sed -n 's/^n//p')"

freed=0
for dir in "$worktrees"/*/; do
  dir="${dir%/}"
  [ -d "$dir/target" ] || continue
  name="$(basename "$dir")"
  if printf '%s\n' "$busy" | grep -q "^$dir\(/\|$\)"; then
    echo "skip   $name (building)"
    continue
  fi
  # Uncommitted work means someone is still at it, merged or not.
  if [ -n "$(git -C "$dir" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    echo "skip   $name (uncommitted changes)"
    continue
  fi
  why=""
  # Merged: behind main, not merely branched from its tip with nothing committed yet.
  if git -C "$dir" merge-base --is-ancestor HEAD origin/main 2>/dev/null \
    && [ "$(git -C "$dir" rev-parse HEAD)" != "$(git -C "$dir" rev-parse origin/main)" ]; then
    why="merged"
  elif [ -n "$idle" ] && [ -z "$(find "$dir/target" -maxdepth 3 -newermt "-${idle} hours" -print -quit 2>/dev/null)" ]; then
    why="idle ${idle}h"
  fi
  [ -n "$why" ] || continue
  kb=$(du -sk "$dir/target" | cut -f1)
  freed=$((freed + kb))
  printf '%-6s %s (%s, %s GB)\n' "$([ $dry = 1 ] && echo would || echo prune)" "$name" "$why" "$((kb / 1048576))"
  [ $dry = 1 ] || rm -rf "$dir/target"
done
echo "$([ $dry = 1 ] && echo would free || echo freed) $((freed / 1048576)) GB"
