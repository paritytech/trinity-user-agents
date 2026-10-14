"""Compare a vendored host tree against the source tree it was imported from.

Reads NUL-delimited `git ls-files -s -z` and `git ls-tree -r -z` output and
reports what the vendored copy is missing, what it carries that the source does
not, and what differs.

NUL-delimited because git quotes and escapes any path outside ASCII, and a
quoted path cannot have a directory prefix pasted onto it.

A path whose content differs has to be explained by an adaptation. One that is
not is upstream work the refresh dropped, which is the case worth failing on.
"""

import sys


def records(path):
    with open(path, "rb") as fh:
        blob = fh.read()
    for record in blob.split(b"\0"):
        if record:
            yield record


def load_index(path):
    """`<mode> <sha> <stage>\\t<path>` from ls-files -s -z."""
    out = {}
    for record in records(path):
        meta, _, name = record.partition(b"\t")
        fields = meta.split()
        if len(fields) >= 2:
            out[name.decode("utf-8", "surrogateescape")] = fields[1].decode()
    return out


def load_tree(path, prefix):
    """`<mode> <type> <sha>\\t<path>` from ls-tree -r -z."""
    out = {}
    for record in records(path):
        meta, _, name = record.partition(b"\t")
        fields = meta.split()
        if len(fields) >= 3 and fields[1] == b"blob":
            out[prefix + name.decode("utf-8", "surrogateescape")] = fields[2].decode()
    return out


def show(label, paths, limit=15):
    print(f"  {label:<28}: {len(paths)}")
    for path in paths[:limit]:
        print(f"      {path}")
    if len(paths) > limit:
        print(f"      ... and {len(paths) - limit} more")


def main(argv):
    ours_path, theirs_path, adapted_path, prefix = argv[1], argv[2], argv[3], argv[4]
    ours = load_index(ours_path)
    theirs = load_tree(theirs_path, prefix)

    missing = sorted(set(theirs) - set(ours))
    extra = sorted(set(ours) - set(theirs))
    differing = sorted(p for p in (set(ours) & set(theirs)) if ours[p] != theirs[p])

    adapted = set()
    try:
        adapted = {
            record.decode("utf-8", "surrogateescape") for record in records(adapted_path)
        }
    except OSError:
        pass

    show("in the source but not here", missing)
    show("here but not in the source", extra)
    show("content differs", differing)

    if missing and not adapted:
        print("  NOTE: a file present upstream and absent here is usually an ignore rule.")

    if not adapted:
        return 0

    unexplained = sorted((set(differing) | set(missing)) - adapted)
    if unexplained:
        print("  UNEXPLAINED, differs from the source with no adaptation to account for it:")
        for path in unexplained[:20]:
            print(f"      {path}")
        return 1

    # The other direction. An adaptation that left the path identical to the
    # source either did not apply, or the source has since adopted it. Which of
    # those it is has to be read, so it is reported rather than failed on.
    inert = sorted(adapted - (set(differing) | set(missing) | set(extra)))
    if inert:
        show("adaptation left no difference", inert)
        print("  Either the source adopted it, or it did not apply. Check before committing.")

    print("  every difference is accounted for by an adaptation")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
