#!/usr/bin/env bash
#
# Refresh a vendored host tree under hosts/<host> from the repository it was
# imported from.
#
# The trees are snapshots. Development continues upstream until a host cuts
# over, and afterwards whenever the source repository keeps a release line of
# its own, so this runs more than once per host.
#
# The new tree is taken whole and this repository's adaptations are re-applied
# on top as a three-way patch, because applying only one of the two discards
# the other side's work.
#
# Paths are compared against the source by blob hash in both directions: a
# difference no adaptation accounts for is upstream work that was dropped, and
# an adaptation that left no difference is an adaptation that did not survive.
#
# Changes move one way, from the source into this tree. A change made here is
# not sent back; the source is upstream of this repository, not a peer.
#
# Usage:
#   scripts/refresh-host-import.sh status <host>
#   scripts/refresh-host-import.sh refresh <host> [--ref <rev>] [--source <url>]
#
# `status` reports drift without touching the tree. `refresh` rewrites
# hosts/<host> in the working tree and leaves the result staged and
# uncommitted, so the resolution of any conflict is a human decision.

set -euo pipefail

readonly MANIFEST="hosts/imports.json"

die() { echo "error: $*" >&2; exit 1; }

# Every mktemp in this script registers here, so a die path leaks nothing. One
# of these can be a large binary patch.
TEMPS=()
cleanup() { [ ${#TEMPS[@]} -eq 0 ] || rm -f "${TEMPS[@]}"; }
trap cleanup EXIT
scratch() { local f; f="$(mktemp)"; TEMPS+=("$f"); printf '%s' "$f"; }
note() { echo "  $*"; }

repo_root() { git rev-parse --show-toplevel; }

manifest_get() {
  local host=$1 field=$2
  python3 - "$host" "$field" <<'PY'
import json, sys
host, field = sys.argv[1], sys.argv[2]
with open("hosts/imports.json") as fh:
    data = json.load(fh)
if host not in data:
    sys.exit(f"unknown host {host!r}; known: {', '.join(sorted(data))}")
print(data[host][field])
PY
}

manifest_set_ref() {
  local host=$1 ref=$2
  python3 - "$host" "$ref" <<'PY'
import json, sys
host, ref = sys.argv[1], sys.argv[2]
path = "hosts/imports.json"
with open(path) as fh:
    data = json.load(fh)
data[host]["ref"] = ref
with open(path, "w") as fh:
    json.dump(data, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY
}

# Fetch a source revision into this repository so both trees are local objects
# and can be diffed without leaving git.
fetch_source() {
  local source=$1 ref=$2
  git fetch --quiet --no-tags "$source" "$ref" 2>/dev/null \
    || git fetch --quiet --no-tags "$source" \
    || die "cannot fetch $source"
  git rev-parse --verify --quiet "${ref}^{commit}" >/dev/null \
    || die "revision $ref is not in $source"
}

# The upstream tree rewritten under hosts/<host>/, as a tree object, so a plain
# `git diff` against our own tree yields a patch that applies at the right path.
upstream_tree_at_prefix() {
  local host=$1 ref=$2 index
  # git tolerates a missing index file but not an empty one, and mktemp leaves
  # a zero byte file behind.
  index="$(mktemp)"
  rm -f "$index"
  GIT_INDEX_FILE="$index" git read-tree --prefix="hosts/${host}/" "${ref}^{tree}"
  GIT_INDEX_FILE="$index" git write-tree
  rm -f "$index"
}

# Every path under hosts/<host>, as "<blobhash> <path>", for the working tree
# and for the source. Comparing these is what catches a file dropped by an
# ignore rule, which a diff of tracked files cannot show.
compare_against_source() {
  local host=$1 ref=$2 adapted_list=$3
  local ours theirs rc
  ours="$(mktemp)"; theirs="$(mktemp)"

  # NUL-delimited, because git quotes and escapes any path outside ASCII and a
  # quoted path cannot have a prefix pasted onto it. One asset in the iOS tree
  # has a Cyrillic character, and with quoted output it reads as both missing
  # and extra.
  git ls-files -s -z "hosts/${host}" > "$ours"
  git ls-tree -r -z "${ref}^{tree}" > "$theirs"

  set +e
  python3 scripts/lib/compare-host-import.py "$ours" "$theirs" "$adapted_list" "hosts/${host}/"
  rc=$?
  set -e
  rm -f "$ours" "$theirs"
  return $rc
}

cmd_status() {
  local host=$1 source ref branch
  source="$(manifest_get "$host" source)"
  ref="$(manifest_get "$host" ref)"
  branch="$(manifest_get "$host" branch)"

  note "host   : hosts/${host}"
  note "source : ${source} (${branch})"
  note "at     : ${ref}"

  fetch_source "$source" "$ref"
  local tip
  tip="$(git ls-remote "$source" "refs/heads/${branch}" | cut -f1)"
  note "tip    : ${tip:-unknown}"

  if [ -n "$tip" ] && [ "$tip" != "$(git rev-parse "${ref}^{commit}")" ]; then
    fetch_source "$source" "$tip"
    note "behind by $(git rev-list --count "${ref}..${tip}" 2>/dev/null || echo '?') commits"
  else
    note "up to date"
  fi

  echo
  note "against the recorded import:"
  compare_against_source "$host" "$ref" /dev/null || true
}

cmd_refresh() {
  local host=$1; shift
  local target="" source=""
  while [ $# -gt 0 ]; do
    case $1 in
      --ref) target=$2; shift 2 ;;
      --source) source=$2; shift 2 ;;
      *) die "unknown argument $1" ;;
    esac
  done

  [ -n "$source" ] || source="$(manifest_get "$host" source)"
  local recorded branch
  recorded="$(manifest_get "$host" ref)"
  branch="$(manifest_get "$host" branch)"

  [ -z "$(git status --porcelain)" ] || die "working tree is dirty; commit or stash first"

  if [ -z "$target" ]; then
    target="$(git ls-remote "$source" "refs/heads/${branch}" | cut -f1)"
    [ -n "$target" ] || die "cannot resolve ${branch} on ${source}"
  fi

  note "refreshing hosts/${host}"
  note "from ${recorded}"
  note "to   ${target}"

  fetch_source "$source" "$recorded"
  fetch_source "$source" "$target"

  if [ "$(git rev-parse "${recorded}^{commit}")" = "$(git rev-parse "${target}^{commit}")" ]; then
    note "already at that revision, nothing to do"
    return 0
  fi

  # What this repository changed relative to the tree it imported.
  local base_tree patch
  base_tree="$(upstream_tree_at_prefix "$host" "$recorded")"
  patch="$(scratch)"
  local deleted_list
  deleted_list="$(scratch)"
  # --binary, or a patch touching a binary file is rejected outright and git
  # apply, which is all or nothing, then applies none of it.
  #
  # Deletions are excluded here and re-applied separately below. git apply
  # cannot three-way a whole-file deletion, because there is no post-image to
  # merge against, so it hard-rejects the moment upstream touches a path this
  # repository deleted. Kept in this patch, that one rejection would discard
  # every other adaptation with it.
  #
  # --no-renames throughout. A moved file is otherwise reported as a rename,
  # which the deletion filter does not see and --name-only lists by its new
  # path alone, so the old path is neither removed nor accounted for and reads
  # as upstream work that was dropped.
  git diff --no-renames --binary --diff-filter=d "$base_tree" HEAD -- "hosts/${host}" > "$patch"
  git diff --no-renames -z --name-only --diff-filter=D "$base_tree" HEAD -- "hosts/${host}" > "$deleted_list"
  local adapted_list
  adapted_list="$(scratch)"
  # NUL-delimited to match the listings it is compared against. --name-only
  # quotes and escapes any path outside ASCII, and one asset in the iOS tree
  # would then never match its own entry and read as dropped work.
  git diff --no-renames -z --name-only "$base_tree" HEAD -- "hosts/${host}" > "$adapted_list"
  note "adaptations to re-apply: $(tr -cd '\0' < "$adapted_list" | wc -c | tr -d ' ') files"

  # Tracked paths only. rm -rf would also take ignored working files such as
  # hosts/ios/source_packages or hosts/android/local.properties, which the
  # dirty check cannot see. read-tree writes through ignore rules, which is
  # what reproducing the source's committed content requires.
  git rm -r -q --ignore-unmatch "hosts/${host}"
  git read-tree --prefix="hosts/${host}/" -u "${target}^{tree}"

  echo
  note "re-applying adaptations"
  if git apply --3way --whitespace=nowarn "$patch"; then
    # No staging here. --3way has already updated the index for every path it
    # touched, and a forced add would sweep in whatever the developer has
    # ignored under this tree, which on these hosts includes plist and env
    # files that must never be committed.
    note "applied cleanly"
  else
    # Staging here would clear the unmerged entries and commit the conflict
    # markers as ordinary content. Leave them unmerged so git refuses.
    local unmerged
    unmerged="$(git diff --name-only --diff-filter=U)"
    if [ -z "$unmerged" ]; then
      # git apply is all or nothing. No unmerged paths after a failure means
      # the patch was rejected outright and the tree is now plain upstream with
      # every adaptation gone, which looks like a clean refresh.
      die "the patch was rejected outright, so no adaptation was applied. The tree is now unmodified upstream and must not be committed. Recover with: git reset --hard HEAD"
    fi
    note "conflicts, left unmerged:"
    printf '%s\n' "$unmerged" | sed 's/^/      /'
    note "resolve them, then stage and commit"
  fi

  # Removing a path says the same thing whether the source kept it, changed it
  # or deleted it too, so this needs no merge and cannot be rejected.
  if [ -s "$deleted_list" ]; then
    # -f because read-tree has just staged the source's version of these
    # paths, and git rm refuses a path whose index entry differs from HEAD.
    # Discarding that staged content is the whole point.
    xargs -0 git rm -r -q -f --ignore-unmatch -- < "$deleted_list"
    note "re-applied $(tr -cd '\0' < "$deleted_list" | wc -c | tr -d ' ') deletions"
  fi

  # The comparison below cannot see this: a resurrected path matches the source
  # exactly, so it reads as an adaptation that left no difference rather than as
  # one that was dropped.
  local resurrected=0
  while IFS= read -r -d '' path; do
    if git ls-files --error-unmatch -- "$path" >/dev/null 2>&1; then
      [ "$resurrected" -ne 0 ] || note "deleted paths that came back:"
      resurrected=$((resurrected + 1))
      note "    ${path}"
    fi
  done < "$deleted_list"
  if [ "$resurrected" -ne 0 ]; then
    die "${resurrected} path(s) this repository deleted are back in the index. Recover with: git reset --hard HEAD"
  fi

  manifest_set_ref "$host" "$target"
  git add "$MANIFEST"
  echo
  note "recorded ${target} in ${MANIFEST}"

  echo
  note "checking the result against the source"
  local verdict=0
  compare_against_source "$host" "$target" "$adapted_list" || verdict=$?

  echo
  if [ "$verdict" -ne 0 ]; then
    die "the refresh dropped work no adaptation accounts for; do not commit it"
  fi
  note "the refresh is staged and uncommitted; review it, then commit"
}

main() {
  cd "$(repo_root)"
  [ -f "$MANIFEST" ] || die "no ${MANIFEST} in this repository"
  local cmd=${1:-}; shift || true
  case "$cmd" in
    status)  [ $# -ge 1 ] || die "usage: $0 status <host>"; cmd_status "$@" ;;
    refresh) [ $# -ge 1 ] || die "usage: $0 refresh <host> [--ref <rev>]"; cmd_refresh "$@" ;;
    *) die "usage: $0 {status|refresh} <host> [options]" ;;
  esac
}

main "$@"
