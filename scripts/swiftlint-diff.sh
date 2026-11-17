#!/usr/bin/env bash
#
# Fails when SwiftLint reports a violation the committed warning baseline does
# not list.
#
# Takes a `swiftlint lint --reporter json` report, the directory it was linted
# from, and the iOS app's build-logs/warnings.json, whose SwiftLint entries
# carry SwiftLint's own message format. Both sides are keyed the way the app
# build's warning ratchet keys them, on path and message with digit runs
# collapsed, so a line shift or a new count in the message ("currently
# contains 508") does not read as a new violation. Multiplicity is kept, so a
# second instance of a listed violation in the same file still counts as new,
# while repeats on one line count once, as the app build counts them. The
# baseline holds only warnings, so an error's message matching a listed
# warning still counts as new.
#
# Accepting a violation, or moving a file that has one, means updating
# warnings.json, the same as for the app build.
#
# Usage:
#   swiftlint-diff.sh --report report.json --root DIR \
#                     --baseline warnings.json [--tree hosts/ios]
#
# Requires: jq.
set -euo pipefail

report="" root="" baseline="" tree=""
while [ $# -gt 0 ]; do
  case "$1" in
    --report) report="$2"; shift 2 ;;
    --root) root="$2"; shift 2 ;;
    --baseline) baseline="$2"; shift 2 ;;
    --tree) tree="$2"; shift 2 ;;
    *) echo "swiftlint-diff: unknown argument '$1'" >&2; exit 2 ;;
  esac
done
for arg in report root baseline; do
  if [ -z "${!arg}" ]; then
    echo "swiftlint-diff: --${arg} is required" >&2
    exit 2
  fi
done

# SwiftLint reports absolute paths.
root="$(cd "$root" && pwd)"
prefix="${tree:+${tree%/}/}"

result="$(jq -n \
  --slurpfile report "$report" --slurpfile baseline "$baseline" \
  --arg root "$root/" '
  def key: [.path, (.message | gsub("[0-9]+"; "#"))] | tojson;

  ($baseline[0].warnings
    | map(select(.path != "" and (.message | test(" Violation: .*\\([a-z_]+\\)$"))))) as $listed
  | ($report[0] | map({
      path: (.file | ltrimstr($root)),
      line: .line,
      message: "\(.type) Violation: \(.reason) (\(.rule_id))",
      error: (.severity == "Error")
    }) | unique_by([.path, .line, .message])) as $found
  | ($listed | map(key) | group_by(.) | map({key: .[0], value: length}) | from_entries) as $counts
  | {
      listed: ($listed | length),
      found: ($found | length),
      added: (($found | map(select(.error)))
              + ($found | map(select(.error | not)) | group_by(key)
                 | map(sort_by(.line) | .[($counts[.[0] | key] // 0):]) | flatten)
              | sort_by(.path, .line))
    }')"

jq -r '"listed=\(.listed) found=\(.found) added=\(.added | length)"' <<<"$result"
jq -r --arg p "$prefix" '.added[]
  | "::error file=\($p)\(.path),line=\(.line // 1)::\(.message)"' <<<"$result"

added="$(jq '.added | length' <<<"$result")"
if [ "$added" -gt 0 ]; then
  echo "${added} SwiftLint violation(s) not in the warning baseline."
  exit 1
fi
echo "No SwiftLint violations beyond the warning baseline."
