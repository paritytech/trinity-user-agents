#!/usr/bin/env bash
#
# Install the truapi-host CLI:
#
#   curl -fsSL https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh | bash
#
# Everything lives in functions that `main` at the very bottom invokes, so a
# download truncated mid-transfer cannot execute a partial install.
#
# Overrides:
#   TRUAPI_HOST_VERSION           install this version instead of the current stable one
#   TRUAPI_HOST_INSTALL_DIR       version store (default $XDG_DATA_HOME/truapi-host)
#   TRUAPI_HOST_BIN_DIR           directory the PATH symlink goes in (default ~/.local/bin)
#   TRUAPI_HOST_RELEASE_BASE_URL  release host, for mirrors and tests
#
# Pass --uninstall to remove an install this script created.

set -euo pipefail

BINARY="truapi-host"
CRATE="truapi-host-cli"
STABLE_TAG="truapi-host-cli-stable"
DEFAULT_BASE_URL="https://github.com/paritytech/trinity-user-agents"

# Global so the EXIT trap can still see it once main's locals are gone.
WORK_DIR=""

cleanup() {
    if [ -n "$WORK_DIR" ]; then
        rm -rf "$WORK_DIR"
    fi
}
trap cleanup EXIT

die() {
    echo "truapi-host-installer: $*" >&2
    exit 1
}

# The release tag is "@parity/truapi@<version>"; both "@" and "/" have to be
# percent-encoded to address its assets. Mirrors ios/truapi-host/scripts/publish.sh.
release_asset_url() {
    local version="$1" name="$2" base
    base="${TRUAPI_HOST_RELEASE_BASE_URL:-$DEFAULT_BASE_URL}"
    printf '%s/releases/download/%%40parity%%2Ftruapi%%40%s/%s' "$base" "$version" "$name"
}

# The trailing query is a cache buster: GitHub's asset CDN keeps serving a
# replaced asset for some minutes, and a no-cache request header does not
# defeat it, so a fresh release would otherwise go unnoticed.
stable_version_url() {
    printf '%s/releases/download/%s/version?t=%s' \
        "${TRUAPI_HOST_RELEASE_BASE_URL:-$DEFAULT_BASE_URL}" "$STABLE_TAG" "$(date +%s)"
}

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os $arch" in
        "Darwin arm64") echo "aarch64-apple-darwin" ;;
        "Linux x86_64") echo "x86_64-unknown-linux-musl" ;;
        "Linux aarch64" | "Linux arm64") echo "aarch64-unknown-linux-musl" ;;
        *)
            die "unsupported platform: $os $arch. Build from source instead: https://github.com/paritytech/trinity-user-agents/blob/main/rust/crates/truapi-host-cli/README.md"
            ;;
    esac
}

download() {
    local url="$1" destination="$2" pinned=""
    # Redirects are followed, so without pinning the transport a redirect to
    # plain http would be taken. Applied to https URLs only, because
    # TRUAPI_HOST_RELEASE_BASE_URL exists for local mirrors and tests that are
    # deliberately http.
    case "$url" in
        https://*) pinned="yes" ;;
    esac

    if command -v curl >/dev/null 2>&1; then
        if [ -n "$pinned" ]; then
            curl -fsSL --proto '=https' --proto-redir '=https' -o "$destination" "$url"
        else
            curl -fsSL -o "$destination" "$url"
        fi || die "could not download $url"
    elif command -v wget >/dev/null 2>&1; then
        if [ -n "$pinned" ]; then
            wget -q --https-only -O "$destination" "$url"
        else
            wget -q -O "$destination" "$url"
        fi || die "could not download $url"
    else
        die "neither curl nor wget is available"
    fi
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        die "neither sha256sum nor shasum is available"
    fi
}

verify_checksum() {
    local archive="$1" checksum_file="$2" expected actual
    expected="$(cut -d' ' -f1 <"$checksum_file")"
    actual="$(sha256_of "$archive")"
    [ -n "$expected" ] || die "published checksum is empty"
    [ "$expected" = "$actual" ] \
        || die "checksum mismatch: expected $expected, got $actual"
}

# Unpack into the version store and point `current` at it. The PATH symlink
# goes through `current`, so later installs and background updates only ever
# move that one link.
activate_version() {
    local root="$1" version="$2" staging="$3" bin_dir="$4"
    local version_dir="$root/versions/$version"

    [ -f "$staging/$BINARY" ] || die "archive did not contain $BINARY"
    chmod +x "$staging/$BINARY"

    mkdir -p "$root/versions" "$bin_dir"
    rm -rf "$version_dir"
    mv "$staging" "$version_dir"

    ln -sfn "versions/$version" "$root/current"
    ln -sfn "$root/current/$BINARY" "$bin_dir/$BINARY"
}

# True when `link` is a symlink pointing inside `root`, so an unrelated
# truapi-host on the PATH is never removed.
links_into() {
    local link="$1" root="$2"
    [ -L "$link" ] || return 1
    case "$(readlink "$link")" in
        "$root"/*) return 0 ;;
        *) return 1 ;;
    esac
}

remove_prebuilt_install() {
    local root="$1" bin_dir="$2" removed=""
    if links_into "$bin_dir/$BINARY" "$root"; then
        rm -f "$bin_dir/$BINARY"
        removed="yes"
    fi
    if [ -d "$root/versions" ] || [ -L "$root/current" ]; then
        rm -rf "$root/versions" "$root/current" \
            "$root/update-check.json" "$root/update.lock"
        removed="yes"
    fi
    if [ -n "$removed" ]; then
        echo "Removed the prebuilt $BINARY install from $root."
    fi
    return 0
}

# A cargo-installed copy and this one shadow each other depending on PATH
# order, so only one of the two should ever exist.
remove_cargo_install() {
    local cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin/$BINARY"
    [ -e "$cargo_bin" ] || return 0
    if command -v cargo >/dev/null 2>&1 && cargo uninstall "$CRATE" >/dev/null 2>&1; then
        echo "Removed the cargo-installed $BINARY."
    else
        rm -f "$cargo_bin"
        echo "Removed $cargo_bin."
    fi
    return 0
}

report() {
    local version="$1" bin_dir="$2"
    echo "Installed $BINARY $version to $bin_dir/$BINARY"
    case ":$PATH:" in
        *":$bin_dir:"*) ;;
        *)
            echo
            echo "$bin_dir is not on your PATH. Add it with:"
            echo "  export PATH=\"$bin_dir:\$PATH\""
            ;;
    esac
    echo
    echo "Run '$BINARY --help' to get started, or '$BINARY signing-host' to"
    echo "start a wallet-local host. Product scripts (--script) also need 'bun'"
    echo "on your PATH; see the truapi-host-cli README."
}

main() {
    local target version root bin_dir work archive checksum

    root="${TRUAPI_HOST_INSTALL_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/truapi-host}"
    bin_dir="${TRUAPI_HOST_BIN_DIR:-$HOME/.local/bin}"

    if [ "${1:-}" = "--uninstall" ]; then
        remove_prebuilt_install "$root" "$bin_dir"
        return 0
    fi

    target="$(detect_target)"
    WORK_DIR="$(mktemp -d)"
    work="$WORK_DIR"

    if [ -n "${TRUAPI_HOST_VERSION:-}" ]; then
        version="$TRUAPI_HOST_VERSION"
    else
        download "$(stable_version_url)" "$work/version"
        version="$(tr -d '[:space:]' <"$work/version")"
        [ -n "$version" ] || die "the stable release pointer is empty"
    fi

    local name="$BINARY-$version-$target.tar.gz"
    archive="$work/$name"
    checksum="$work/$name.sha256"

    echo "Downloading $BINARY $version for $target..."
    download "$(release_asset_url "$version" "$name")" "$archive"
    download "$(release_asset_url "$version" "$name.sha256")" "$checksum"
    verify_checksum "$archive" "$checksum"

    local staging="$work/staging"
    mkdir -p "$staging"
    tar -xzf "$archive" -C "$staging" || die "could not unpack $name"

    activate_version "$root" "$version" "$staging" "$bin_dir"
    remove_cargo_install
    report "$version" "$bin_dir"
}

main "$@"
