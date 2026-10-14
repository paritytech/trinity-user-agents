#!/usr/bin/env bash
# PostToolUse hook for Edit|Write on .kt files.
# Reads the hook event JSON from stdin, examines the touched file,
# and prints rule-aligned warnings to stdout for Claude to consume.
# Exits 0 always (advisory). Use the PreToolUse blocker for hard rules.

set -uo pipefail

input="$(cat)"
tool_name="$(printf '%s' "$input" | jq -r '.tool_name // empty')"
file_path="$(printf '%s' "$input" | jq -r '
    (.tool_input.file_path
        // .tool_input.notebook_path
        // .tool_input.path
        // empty)')"

[[ -z "$file_path" ]] && exit 0
[[ "$file_path" != *.kt && "$file_path" != *.kts ]] && exit 0
[[ ! -f "$file_path" ]] && exit 0

# Skip generated and build outputs
case "$file_path" in
    */build/*|*/generated/*|*/.claude/*) exit 0 ;;
esac

warnings=()

# Helper: append a warning with file:line excerpts.
add_finding() {
    local label="$1"; shift
    local pattern="$1"; shift
    local hits
    hits="$(grep -nE "$pattern" "$file_path" 2>/dev/null || true)"
    if [[ -n "$hits" ]]; then
        warnings+=("⚠ $label:")
        while IFS= read -r line; do
            warnings+=("    $file_path:$line")
        done <<< "$hits"
    fi
}

# Rule citations point reviewers at the matching checklist sections.

# code/results-and-errors.md § getOrThrow — avoid everywhere
# Worker.doWork seam is the only legitimate use; allow it there.
if ! grep -qE '\bclass\b.*:\s*CoroutineWorker\b' "$file_path" \
    && [[ "$file_path" != */src/test/* && "$file_path" != */src/androidTest/* ]]; then
    add_finding "getOrThrow() outside Worker.doWork / tests (code/results-and-errors.md § getOrThrow — avoid everywhere)" '\.getOrThrow\(\)'
fi

# code/ui-compose.md § VerticalSpacer / HorizontalSpacer
add_finding "Spacer(Modifier.height/width) — use VerticalSpacer { spacingN } / HorizontalSpacer { spacingN } (code/ui-compose.md § Spacing)" 'Spacer\(\s*Modifier\.(height|width)\b'

# code/naming-and-hygiene.md § Logging — Timber only
add_finding "android.util.Log.* — use Timber (code/naming-and-hygiene.md § Logging — Timber only)" '\bLog\.[diwev]\('

# code/ui-compose.md § Nova wrappers — never raw Material
add_finding "raw Material import — use Nova* wrapper (code/ui-compose.md § Nova wrappers)" '^import androidx\.compose\.material3?\.(Button|TextButton|TextField|OutlinedTextField|Text|Surface|Icon|AlertDialog|ModalBottomSheet)\b'

# code/results-and-errors.md § runCatching { throw … } ping-pong (heuristic: simple inline pattern)
add_finding "runCatching { throw … } ping-pong — return Result.failure instead (code/results-and-errors.md § Anti-patterns)" 'runCatching\s*\{[^{}]*\bthrow\b'

# code/state-management.md § Shadow the mutable: avoid _state backing fields
add_finding "private _state backing field — shadow the StateFlow with MutableStateFlow directly (code/state-management.md § Shadow the mutable)" '^\s*private\s+val\s+_[a-zA-Z]+\s*=\s*MutableStateFlow'

# project-types-and-units.md § DataByteArray
add_finding "raw ByteArray in @Serializable data class — use DataByteArray (code/project-types-and-units.md § DataByteArray)" 'val\s+[a-zA-Z]+\s*:\s*ByteArray\b'

if (( ${#warnings[@]} > 0 )); then
    printf 'PolkadotApp PostToolUse: project-rule warnings on %s\n' "$file_path"
    printf '%s\n' "${warnings[@]}"
    printf '\nFix these before the next edit. The reviewer will flag them otherwise.\n'
fi

exit 0
