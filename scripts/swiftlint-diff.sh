#!/usr/bin/env bash
#
# Fails when a change adds a SwiftLint violation that its base does not have.
#
# Takes two `swiftlint lint --reporter json` reports, one from the base tree
# and one from the change, and the directory each was linted from. A violation
# is keyed on its file, rule and reason with digit runs collapsed, so that a
# line shift or a new count in the reason ("currently contains 508") does not
# read as a new violation. Multiplicity is kept, so a second instance of an
# existing violation in the same file still counts as added. Files the change
# renames, given as `git diff -M --name-status --diff-filter=R` output, carry
# their base violations to the new path.
#
# Two cases net out or count in full by design. Fixing one violation and
# adding another of the same rule and reason in the same file counts as no
# change. A change that tightens `.swiftlint.yml` counts every existing
# violation of the tightened rule as added, since each tree is linted with its
# own config.
#
# Usage:
#   swiftlint-diff.sh --base base.json --base-root DIR \
#                     --head head.json --head-root DIR \
#                     [--renames renames.tsv] [--tree hosts/ios]
#
# Requires: jq.
set -euo pipefail

base="" base_root="" head="" head_root="" renames="" tree=""
while [ $# -gt 0 ]; do
  case "$1" in
    --base) base="$2"; shift 2 ;;
    --base-root) base_root="$2"; shift 2 ;;
    --head) head="$2"; shift 2 ;;
    --head-root) head_root="$2"; shift 2 ;;
    --renames) renames="$2"; shift 2 ;;
    --tree) tree="$2"; shift 2 ;;
    *) echo "swiftlint-diff: unknown argument '$1'" >&2; exit 2 ;;
  esac
done
for arg in base base_root head head_root; do
  if [ -z "${!arg}" ]; then
    echo "swiftlint-diff: --${arg//_/-} is required" >&2
    exit 2
  fi
done

# SwiftLint reports absolute paths.
base_root="$(cd "$base_root" && pwd)"
head_root="$(cd "$head_root" && pwd)"
prefix="${tree:+${tree%/}/}"

rename_map='{}'
if [ -n "$renames" ]; then
  rename_map="$(jq -R -n --arg p "$prefix" '
    [inputs | split("\t")
      | select(length == 3 and (.[0] | startswith("R"))
               and (.[1] | startswith($p)) and (.[2] | startswith($p)))
      | {key: (.[1] | ltrimstr($p)), value: (.[2] | ltrimstr($p))}]
    | from_entries' "$renames")"
fi

result="$(jq -n \
  --slurpfile base "$base" --slurpfile head "$head" \
  --arg base_root "$base_root/" --arg head_root "$head_root/" \
  --argjson renames "$rename_map" '
  def key: [.path, .rule_id, (.reason | gsub("[0-9]+"; "#"))] | tojson;

  ($base[0] | map(.path = (.file | ltrimstr($base_root))
                  | .path = ($renames[.path] // .path))) as $b
  | ($head[0] | map(.path = (.file | ltrimstr($head_root)))) as $h
  | ($b | map(key) | group_by(.) | map({key: .[0], value: length}) | from_entries) as $counts
  | {
      base: ($b | length),
      head: ($h | length),
      renamed: ($renames | length),
      added: ($h | group_by(key) | map(sort_by(.line) | .[($counts[.[0] | key] // 0):])
              | flatten | sort_by(.path, .line))
    }')"

jq -r '"base=\(.base) head=\(.head) renamed=\(.renamed) added=\(.added | length)"' <<<"$result"
jq -r --arg p "$prefix" '.added[]
  | "::error file=\($p)\(.path),line=\(.line // 1)::\(.type) Violation: \(.reason) (\(.rule_id))"' <<<"$result"

added="$(jq '.added | length' <<<"$result")"
if [ "$added" -gt 0 ]; then
  echo "${added} SwiftLint violation(s) added against the base."
  exit 1
fi
echo "No SwiftLint violations added against the base."
