#!/bin/bash
#
# extract_xcresult.sh
# Locate the build's .xcresult bundle, run xcresulttool, normalize to a
# repo-relative warnings.json, and (optionally) set-diff it against a committed
# baseline to ratchet warnings on CI.
#
# Usage:
#   extract_xcresult.sh --out current.json [--xcresult PATH]
#       Extract only. Writes normalized warnings to --out. Exit 0 on success.
#
#   extract_xcresult.sh --out current.json --baseline build-logs/warnings.json \
#                       [--rdjson added.rdjson] [--xcresult PATH]
#       Extract + ratchet. Compares against --baseline, keyed on
#       (path, message with digit runs normalized) — NOT on line number:
#         - warnings ADDED vs baseline (count rise, or swap at equal count)
#           -> exit 1 (FAIL). Added warnings written to --rdjson (reviewdog
#              rdjson) when --rdjson given.
#         - warnings RESOLVED only, none added -> exit 0 (WARN: baseline stale,
#           nudge to regenerate build-logs/warnings.json via the extract-only form).
#         - identical -> exit 0.
#
# Requires: xcrun (Xcode), python3. No Homebrew deps.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NORMALIZER="$REPO_ROOT/Runscripts/normalize_warnings.py"

OUT=""
BASELINE=""
RDJSON=""
XCRESULT=""

while [ $# -gt 0 ]; do
    case "$1" in
        --out)       OUT="$2";       shift 2 ;;
        --baseline)  BASELINE="$2";  shift 2 ;;
        --rdjson)    RDJSON="$2";     shift 2 ;;
        --xcresult)  XCRESULT="$2";   shift 2 ;;
        *) echo "extract_xcresult: unknown arg '$1'" >&2; exit 2 ;;
    esac
done

if [ -z "$OUT" ]; then
    echo "extract_xcresult: --out is required" >&2
    exit 2
fi

# Locate the .xcresult if not provided. gym (result_bundle: true) writes it to
# the fastlane output directory; fall back to the newest under DerivedData.
if [ -z "$XCRESULT" ]; then
    for cand in \
        "$REPO_ROOT"/fastlane/test_output/*.xcresult \
        "$REPO_ROOT"/*.xcresult \
        "$REPO_ROOT"/fastlane/*.xcresult ; do
        [ -d "$cand" ] && XCRESULT="$cand"
    done
    if [ -z "$XCRESULT" ]; then
        XCRESULT="$(ls -dt "$HOME/Library/Developer/Xcode/DerivedData"/polkadot-app-*/Logs/*/*.xcresult 2>/dev/null | head -1)"
    fi
fi

if [ -z "$XCRESULT" ] || [ ! -d "$XCRESULT" ]; then
    echo "extract_xcresult: no .xcresult bundle found" >&2
    exit 1
fi
echo "extract_xcresult: using $XCRESULT"

# Modern build-results API (Xcode 16+). Legacy `get --format json` is deprecated.
if ! xcrun xcresulttool get build-results --path "$XCRESULT" --format json \
        | "$NORMALIZER" "$REPO_ROOT" > "$OUT"; then
    echo "extract_xcresult: xcresulttool/normalizer failed" >&2
    exit 1
fi

CURRENT_COUNT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["count"])' "$OUT")"
echo "extract_xcresult: $CURRENT_COUNT warning(s) -> $OUT"

# Extract-only mode.
[ -z "$BASELINE" ] && exit 0

if [ ! -f "$BASELINE" ]; then
    echo "extract_xcresult: baseline '$BASELINE' missing — generate one: Runscripts/extract_xcresult.sh --out build-logs/warnings.json" >&2
    exit 1
fi

# Set-diff against baseline. Writes rdjson of added warnings (if requested),
# prints a summary, and exits 1 when anything was added.
python3 - "$BASELINE" "$OUT" "${RDJSON:-}" <<'PY'
import json, re, sys
from collections import Counter

baseline_path, current_path, rdjson_path = sys.argv[1], sys.argv[2], sys.argv[3]

# The key excludes the line number and normalizes digit runs in the message: an
# edit above a warning shifts every line below it, and SwiftLint embeds counts in
# its text ("currently contains 549"). Keying on either reports untouched
# warnings as new on every PR. Counter preserves multiplicity, so a genuinely
# added duplicate of an existing warning still trips the ratchet.
#
# Path-less warnings (CoreData model, build-phase) are compared verbatim: they
# have no line to drift, and their digits are semantic — CoreData's trailing
# "[7]" counts affected relationships, so [7] -> [8] is a new problem, not churn.
def key(w):
    if not w["path"]:
        return ("", w["message"])
    return (w["path"], re.sub(r"\d+", "#", w["message"]))

def load(p):
    warnings = json.load(open(p)).get("warnings", [])
    return Counter(key(w) for w in warnings), warnings

base_counts, base_warnings = load(baseline_path)
cur_counts, cur_warnings = load(current_path)

def pick(warnings, surplus):
    """Representative records for each surplus key, keeping real line numbers."""
    by_key = {}
    for w in warnings:
        by_key.setdefault(key(w), []).append(w)
    out = []
    for k, n in surplus.items():
        out.extend(by_key[k][:n])
    return sorted(out, key=lambda w: (w["path"], w["line"] or 0, w["message"]))

added = pick(cur_warnings, cur_counts - base_counts)
resolved = pick(base_warnings, base_counts - cur_counts)

def loc(w):
    return f"{w['path']}:{w['line']}" if w["path"] else "(no file)"

if rdjson_path and added:
    diags = []
    for w in added:
        if not w["path"]:
            continue  # synthetic (no file) — can't annotate inline
        diags.append({
            "message": w["message"],
            "location": {
                "path": w["path"],
                "range": {"start": {"line": w["line"] or 1, "column": 1}},
            },
            "severity": "WARNING",
        })
    rd = {"source": {"name": "warning-ratchet"}, "diagnostics": diags}
    with open(rdjson_path, "w") as f:
        json.dump(rd, f)

print(f"baseline={len(base_warnings)} current={len(cur_warnings)} added={len(added)} resolved={len(resolved)}")

if added:
    print("::error::Warning ratchet: %d new warning(s) introduced:" % len(added))
    for w in added:
        print(f"  + {loc(w)}: {w['message']}")
    if resolved:
        print("(also %d resolved — regenerate the baseline after fixing the new ones)" % len(resolved))
    sys.exit(1)

if resolved:
    print("::warning::%d warning(s) resolved since baseline. Regenerate & commit build-logs/warnings.json to lower the bar: Runscripts/extract_xcresult.sh --out build-logs/warnings.json" % len(resolved))
    for w in resolved:
        print(f"  - {loc(w)}: {w['message']}")

print("Warning ratchet: OK (no new warnings).")
sys.exit(0)
PY
RATCHET_EXIT=$?
exit $RATCHET_EXIT
