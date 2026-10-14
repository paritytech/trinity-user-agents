#!/usr/bin/env bash
# PreToolUse hook for Edit|Write|MultiEdit.
# Blocks edits to paths that should never be modified in place:
#   - database/schemas/<n>.json  (Room schema files are append-only; bump version, don't edit)
#   - .claude/PLAN.md            (architect skill owns this; implementer should not rewrite it mid-stream)
# Exit 2 to deny the tool call with a message Claude will see.

set -uo pipefail

input="$(cat)"
file_path="$(printf '%s' "$input" | jq -r '
    (.tool_input.file_path
        // .tool_input.notebook_path
        // .tool_input.path
        // empty)')"

[[ -z "$file_path" ]] && exit 0

case "$file_path" in
    */database/schemas/*.json)
        cat <<'EOF' >&2
⛔ Blocked by PolkadotApp policy.

You're about to edit an existing file under database/schemas/. Room schema files
are append-only: a PR must NEVER modify an existing schema file. To change the
database, bump @Database(version = N+1, ...), add a Migration(N, N+1) object,
and let Room generate database/schemas/<N+1>.json on the next build.

See code/database-and-scale.md § Room schema files are append-only.

If you truly need to modify a generated schema (e.g. regenerating after a
mistake), escalate to the user — do NOT proceed via Edit/Write.
EOF
        exit 2
        ;;
esac

exit 0
