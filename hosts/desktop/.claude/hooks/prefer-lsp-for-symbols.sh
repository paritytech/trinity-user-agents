#!/usr/bin/env bash
# PreToolUse hook for Grep and Bash.
#
# Symbol questions belong to the LSP tool: it resolves through the barrel
# re-exports that every domain in this repo uses as its public surface, and it
# excludes README prose and vi.mock properties. Grep does neither, so a grep
# that "finds nothing" is routinely a symbol that is very much alive behind an
# index.ts — the exact mistake this gate exists to stop.
#
# Covers BOTH entry points. The Grep tool is the obvious one; `Bash(grep ...)`
# is the one that actually gets used, and before 2026-08 it bypassed this hook
# entirely because the matcher only listed Grep.
#
# Deliberately narrow. Fires ONLY on an identifier with an internal case
# transition (productWorker, useProductWorker, ProductWorkerInstance). Free text
# ("durable stream", "TODO"), single dictionary words ("relay", "Repository"),
# and anything carrying regex metacharacters pass through untouched — grep stays
# the right tool for those.
#
# For Bash it inspects only QUOTED arguments of a search command, so unquoted
# paths (src/domains/chat/p2p/managerV2Factory.ts) never trip it, and commands
# that don't invoke a search tool at all are ignored.
#
# DENIES rather than nudges. A nudge arrives attached to the result it was meant
# to prevent, which is too late to change the decision. Escape hatch: put the
# literal marker `# free-text` anywhere in the command to assert this is genuine
# text matching (filtering command output, searching prose) and pass through.

set -uo pipefail

input="$(cat)"
tool="$(printf '%s' "$input" | jq -r '.tool_name // empty')"

case "$tool" in
  Grep)
    candidates="$(printf '%s' "$input" | jq -r '.tool_input.pattern // empty')"
    ;;
  Bash)
    cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
    [[ "$cmd" == *"# free-text"* ]] && exit 0
    # Only search commands are in scope.
    printf '%s' "$cmd" | grep -Eq '(^|[|;&(]|[[:space:]])(grep|egrep|fgrep|rg|ag)([[:space:]]|$)' || exit 0
    # Quoted arguments only — patterns get quoted, paths and flags usually don't.
    candidates="$(printf '%s' "$cmd" | grep -oE "'[^']*'|\"[^\"]*\"" | sed -E "s/^['\"]//; s/['\"]\$//")"
    ;;
  *)
    exit 0
    ;;
esac

[[ -z "$candidates" ]] && exit 0

# Split alternations so a multi-symbol search ("foo\|bar") is still caught —
# that shape is how most real searches are written, and testing the whole
# string as one identifier would miss every one of them.
symbol=""
while IFS= read -r branch; do
  [[ -z "$branch" ]] && continue
  [[ "$branch" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]] || continue
  [[ "$branch" =~ [a-z][A-Z] ]] || continue
  symbol="$branch"
  break
done < <(printf '%s\n' "$candidates" | sed -E 's/\\\|/\n/g; s/\|/\n/g')

[[ -z "$symbol" ]] && exit 0

read -r -d '' reason <<EOF || true
'$symbol' is a code symbol, so grep is the wrong tool here and this call is blocked.

Use LSP instead:
  - who calls / uses it  -> findReferences (or incomingCalls)
  - where is it defined  -> goToDefinition
  - what is its type     -> hover
  - find it by name      -> workspaceSymbol

Position is 1-based and must land on the identifier itself, not the line start.
LSP is a deferred tool: if it is not in your tool list, load it once with
ToolSearch("select:LSP") first.

Why this is blocked and not merely suggested: grep does not follow the barrel
index.ts re-exports this repo mandates as every domain's public surface, and it
counts README prose and vi.mock properties as hits. A grep that looks empty is
routinely a symbol with live callers.

If this really is text matching and not a symbol search — filtering command
output, searching docs or prose — add the literal marker '# free-text' to the
command and re-run.
EOF

jq -n --arg reason "$reason" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: $reason
  }
}'

exit 0
