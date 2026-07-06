#!/usr/bin/env bash
#
# install-agents-snippet.sh — wire Sweetgrass knowledge capture into a
# project's AGENTS.md and CLAUDE.md.
#
# Those files are user-specific (gitignored) in beads orchestration projects,
# so the canonical snippet lives in docs/agents-snippet.md and this script
# installs it locally.
#
# Usage: scripts/install-agents-snippet.sh [target-project-root]
#   Default target: the repo root of the current directory.
#
# Idempotent: replaces an existing SWEETGRASS KNOWLEDGE CAPTURE block
# (matched by its BEGIN/END markers) or appends one; creates
# AGENTS.md/CLAUDE.md if missing.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SNIPPET_SRC="$SCRIPT_DIR/../docs/agents-snippet.md"
BEGIN_MARKER="<!-- BEGIN SWEETGRASS KNOWLEDGE CAPTURE"
END_MARKER="<!-- END SWEETGRASS KNOWLEDGE CAPTURE -->"

TARGET_ROOT="${1:-$(git rev-parse --show-toplevel)}"

[[ -f "$SNIPPET_SRC" ]] || { echo "snippet source not found: $SNIPPET_SRC" >&2; exit 1; }
[[ -d "$TARGET_ROOT" ]] || { echo "target root not found: $TARGET_ROOT" >&2; exit 1; }

# Extract the marker-delimited block (inclusive) from the snippet doc.
# NOTE: the snippet body is never passed through `awk -v` — that would
# mangle backslash escapes in the embedded curl example.
SNIPPET="$(awk -v b="$BEGIN_MARKER" -v e="$END_MARKER" '
  index($0, b) { inblock = 1 }
  inblock { print }
  index($0, e) { exit }
' "$SNIPPET_SRC")"

[[ -n "$SNIPPET" ]] || { echo "no marker block found in $SNIPPET_SRC" >&2; exit 1; }

for name in AGENTS.md CLAUDE.md; do
  file="$TARGET_ROOT/$name"

  existing=""
  action="created"
  if [[ -f "$file" ]]; then
    if grep -qF "$BEGIN_MARKER" "$file"; then
      # Keep everything except the old block. `$( )` trims trailing newlines,
      # so re-runs do not accumulate blank lines.
      existing="$(awk -v b="$BEGIN_MARKER" -v e="$END_MARKER" '
        index($0, b) { inblock = 1; next }
        inblock && index($0, e) { inblock = 0; next }
        !inblock { print }
      ' "$file")"
      action="updated block in"
    else
      existing="$(cat "$file")"
      action="appended block to"
    fi
  fi

  {
    if [[ -n "${existing//[[:space:]]/}" ]]; then
      printf '%s\n\n' "$existing"
    fi
    printf '%s\n' "$SNIPPET"
  } > "$file"
  echo "$action $file"
done
