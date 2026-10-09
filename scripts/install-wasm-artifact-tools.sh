#!/usr/bin/env bash
# wasm-pack remains responsible for provider/verifiable builds. The core builds
# explicit cdylib artifacts so its protocol can also be linked by no_std guests.
set -euo pipefail
cd "$(dirname "$0")/.."

bindgen_version=$(node -e '
  const { execFileSync } = require("node:child_process");
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"], { maxBuffer: 32 * 1024 * 1024 }));
  console.log(metadata.packages.find(pkg => pkg.name === "wasm-bindgen").version);
')
cargo install wasm-bindgen-cli --version "$bindgen_version" --locked

# Match wasm-pack 0.14.0, which builds the other published WASM modules.
version=117
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) platform=x86_64-linux ;;
  Linux/aarch64) platform=aarch64-linux ;;
  Darwin/arm64) platform=arm64-macos ;;
  Darwin/x86_64) platform=x86_64-macos ;;
  *) echo "Unsupported Binaryen platform" >&2; exit 1 ;;
esac
archive="binaryen-version_${version}-${platform}.tar.gz"
url="https://github.com/WebAssembly/binaryen/releases/download/version_${version}"
destination="${HOME}/.local/share/truapi-wasm-tools"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl --fail --location --silent --show-error "$url/$archive" -o "$tmp/$archive"
curl --fail --location --silent --show-error "$url/$archive.sha256" -o "$tmp/$archive.sha256"
(cd "$tmp" && shasum -a 256 --check "$archive.sha256")
mkdir -p "$destination"
tar -xzf "$tmp/$archive" -C "$destination"
bin="$destination/binaryen-version_${version}/bin"
if [[ -n "${GITHUB_PATH:-}" ]]; then
  printf '%s\n' "$bin" >> "$GITHUB_PATH"
fi
printf 'WASM artifact tools installed. Add %s to PATH.\n' "$bin"
