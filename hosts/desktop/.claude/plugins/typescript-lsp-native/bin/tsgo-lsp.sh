#!/usr/bin/env bash
# Launches the project's TypeScript 7 binary in LSP mode.
#
# `typescript` in this repo is aliased to @typescript/typescript6, so
# node_modules/typescript/lib/tsserver.js is the OLD Node implementation.
# `@typescript/native` (typescript@7.0.2) is the Go port — the same engine
# WebStorm runs — and its `tsc` binary doubles as an LSP server via --lsp.
#
# Verified equivalent to typescript-language-server 5.3.0: both return the
# same 12 references for createProductWorker, including the caller reached
# through the barrel re-export in src/domains/product/index.ts.

set -euo pipefail

root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"
bin="$root/node_modules/.bin/tsc"

if [[ ! -x "$bin" ]]; then
    echo "tsgo-lsp: $bin not found — run 'npm ci' first." >&2
    exit 1
fi

exec "$bin" --lsp -stdio
