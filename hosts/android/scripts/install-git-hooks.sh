#!/bin/bash

# Install git hooks

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "📌 Installing git hooks..."
cp "$SCRIPT_DIR/git-hooks/pre-commit" "$PROJECT_ROOT/.git/hooks/pre-commit"
chmod +x "$PROJECT_ROOT/.git/hooks/pre-commit"
echo "   ✅ pre-commit hook installed"
