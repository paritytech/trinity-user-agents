#!/usr/bin/env bash
#
# One-time local bootstrap for a fresh checkout. Scaffolds the gitignored config
# files from their committed templates and generates Secrets.generated.swift so
# the project compiles and runs with safe public defaults. Safe to re-run; it
# never overwrites files that already exist.
#
#     ./Scripts/setup-secrets.sh
#
# Everything it scaffolds is optional to fill in:
#   - polkadot-app/env-vars.sh          empty values simply disable that feature
#   - GoogleService-Info-*.plist        placeholder Firebase config (Remote Config
#                                        is inert until you drop in real plists)
#
# See docs/PUBLISHING.md for the full configuration and publishing guide.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$ROOT_DIR/polkadot-app"
GS_DIR="$APP_DIR/GoogleService"

scaffold() {
    local template="$1" target="$2"
    if [ -f "$target" ]; then
        echo "✓ $(basename "$target") already exists — leaving untouched"
    elif [ -f "$template" ]; then
        cp "$template" "$target"
        echo "→ created $(basename "$target") from $(basename "$template")"
    else
        echo "⚠ template missing: $template" >&2
    fi
}

scaffold "$APP_DIR/env-vars.template.sh" "$APP_DIR/env-vars.sh"
scaffold "$GS_DIR/GoogleService-Info-Dev.plist.template" "$GS_DIR/GoogleService-Info-Dev.plist"
scaffold "$GS_DIR/GoogleService-Info-Release.plist.template" "$GS_DIR/GoogleService-Info-Release.plist"
scaffold "$GS_DIR/GoogleService-Info-Safety.plist.template" "$GS_DIR/GoogleService-Info-Safety.plist"

echo ""
echo "Generating Secrets.generated.swift..."
"$SCRIPT_DIR/generate_secrets.sh"

echo ""
echo "Done. Next (all optional):"
echo "  1. Edit polkadot-app/env-vars.sh with real secrets — empty values keep the feature disabled."
echo "  2. Replace the GoogleService-Info-*.plist placeholders with plists from your Firebase project."
echo "  3. See docs/PUBLISHING.md before distributing a build."
