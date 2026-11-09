#!/usr/bin/env bash
# Bundles the reference funding provider into this app: see
# scripts/bundle-funding-provider.sh at the repository root, shared by both hosts.
#
# Usage: [FUNDTEST_DIR=… | FUNDTEST_REF=…] ios/Runscripts/bundle-funding-provider.sh [--clean]
exec "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)/scripts/bundle-funding-provider.sh" ios "$@"
