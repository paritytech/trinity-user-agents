#!/usr/bin/env python3
"""Hold the labelling and the reviewer rules to the same set of paths.

`.github/labeler.yml` decides which labels a pull request gets, and
`.github/CODEOWNERS` decides who is asked to review it. Both answer the same
question, which paths belong to which area, and neither format can express the
other, so they are two files. Two files drift.

This compares the roots each one names. A path that is labelled but owned by
nobody in particular, or owned but unlabelled, is reported. It does not compare
glob semantics, only that both files know about the same places.
"""

import pathlib
import re
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
LABELER = ROOT / ".github" / "labeler.yml"
CODEOWNERS = ROOT / ".github" / "CODEOWNERS"

# Areas the labeller describes by file suffix or by a directory that CODEOWNERS
# covers through a parent entry. Listing them keeps the check honest about what
# it is choosing not to compare.
EXEMPT = {
    "*.md",            # matched anywhere, not a root
    "docs/rfcs/**",    # inside /docs/, owned by the entry for it
}


def root_of(glob: str) -> str:
    """The leading directory a glob constrains, as CODEOWNERS would spell it."""
    head = glob.split("*", 1)[0].rstrip("/")
    return f"/{head}/" if head else ""


def labeller_roots() -> set[str]:
    data = yaml.safe_load(LABELER.read_text())
    roots = set()
    for rules in data.values():
        for rule in rules:
            for globs in rule.get("changed-files", []):
                for glob in globs.get("any-glob-to-any-file", []):
                    if glob in EXEMPT:
                        continue
                    root = root_of(glob)
                    if root:
                        roots.add(root)
    return roots


def codeowner_roots() -> set[str]:
    roots = set()
    for line in CODEOWNERS.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        pattern = re.split(r"\s+", line)[0]
        if pattern == "*":
            continue
        roots.add(pattern if pattern.endswith("/") else pattern + "/")
    return roots


def main() -> int:
    labelled = labeller_roots()
    owned = codeowner_roots()

    unowned = sorted(labelled - owned)
    unlabelled = sorted(owned - labelled)

    for path in unowned:
        print(f"::error::{path} is labelled but names no owner in CODEOWNERS")
    for path in unlabelled:
        print(f"::error::{path} names an owner but is not labelled in labeler.yml")

    if unowned or unlabelled:
        print("\nBoth files answer which paths belong to which area. "
              "Add the path to the other, or add it to EXEMPT here with a reason.")
        return 1

    print(f"labelling and ownership agree on {len(labelled)} paths")
    return 0


if __name__ == "__main__":
    sys.exit(main())
