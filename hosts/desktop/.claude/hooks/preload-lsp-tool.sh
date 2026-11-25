#!/usr/bin/env bash
# SessionStart hook.
#
# The LSP tool is deferred: it is absent from the model's tool list until a
# ToolSearch loads its schema. Bash grep, by contrast, is always resident and
# costs nothing. That asymmetry — not the written rule in CLAUDE.md — is what
# decides which tool gets reached for mid-task, and it is why symbol searches
# kept going to grep despite the rule.
#
# Loading LSP up front removes the asymmetry, so the PreToolUse deny in
# prefer-lsp-for-symbols.sh has somewhere cheap to redirect to.

set -uo pipefail

read -r -d '' context <<'EOF' || true
Code navigation in this repo goes through the LSP tool, which is DEFERRED — load it now, before any code search, with:

  ToolSearch("select:LSP")

Then use it for every symbol question: findReferences / incomingCalls (who uses this), goToDefinition (where is it), hover (what type is it), workspaceSymbol (find by name). Position is 1-based and must land on the identifier.

A PreToolUse hook BLOCKS grep — both the Grep tool and Bash grep/rg — when the pattern is a code identifier, because grep does not follow the barrel index.ts re-exports this repo uses as every domain's public surface. Grep remains correct for free text, filenames, and non-code files.
EOF

jq -n --arg context "$context" '{
  hookSpecificOutput: {
    hookEventName: "SessionStart",
    additionalContext: $context
  }
}'
