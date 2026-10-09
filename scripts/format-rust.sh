#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 1 || (${1:-} != "" && ${1:-} != --check) ]]; then
  echo "Usage: $0 [--check]" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < nightly-toolchain)"

cargo +"$NIGHTLY_TOOLCHAIN" fmt --all "$@"

# cargo fmt cannot discover the modules declared inside runtime_items!.
files=()
while IFS= read -r -d '' file; do
  files+=("$file")
done < <(git ls-files --cached --others --exclude-standard -z -- 'rust/crates/truapi/src/*.rs')

rustfmt +"$NIGHTLY_TOOLCHAIN" --edition 2024 --config skip_children=true "$@" "${files[@]}"
