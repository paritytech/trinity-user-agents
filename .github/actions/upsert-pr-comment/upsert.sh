#!/usr/bin/env bash
#
# Keep exactly one marker-tagged comment on a pull request. Its own file
# rather than an inline run: block, because actionlint shellchecks workflows
# and not a composite action's steps, so inline shell here would be the only
# shell in the repository outside the lint gate.
#
# Reads MARKER, BODY_FILE, PR, REPO and GH_TOKEN from the environment.

set -euo pipefail

[ -f "${BODY_FILE}" ] || { echo "nothing to post"; exit 0; }

body="$(mktemp)"
{ echo "${MARKER}"; cat "${BODY_FILE}"; } > "${body}"

# Slurped across pages: a match can be on any page, and the newest is
# the one worth keeping.
ids="$(gh api "repos/${REPO}/issues/${PR}/comments" --paginate \
  | jq -s --arg m "${MARKER}" '[.[][] | select(.body | startswith($m)) | .id]')"

if [ "$(jq 'length' <<<"${ids}")" -eq 0 ]; then
  gh api --method POST "repos/${REPO}/issues/${PR}/comments" \
    -F body=@"${body}" >/dev/null
  echo "created a comment"
  exit 0
fi

keep="$(jq -r 'last' <<<"${ids}")"
gh api --method PATCH "repos/${REPO}/issues/comments/${keep}" \
  -F body=@"${body}" >/dev/null
echo "updated comment ${keep}"

for stale in $(jq -r '.[:-1][]' <<<"${ids}"); do
  gh api --method DELETE "repos/${REPO}/issues/comments/${stale}" >/dev/null
  echo "deleted duplicate comment ${stale}"
done
