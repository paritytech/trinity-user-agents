#!/usr/bin/env python3
"""Fail when a change adds a SwiftLint violation that its base does not have.

Takes two `swiftlint lint --reporter json` reports, one from the base tree and
one from the change, and the directory each was linted from. A violation is
keyed on its file, rule and reason with digit runs collapsed, so that a line
shift or a new count in the reason ("currently contains 508") does not read as
a new violation. Multiplicity is kept, so a second instance of an existing
violation in the same file still counts as added. Files the change renames,
given as `git diff -M --name-status --diff-filter=R` output, carry their base
violations to the new path.

Two cases net out or count in full by design. Fixing one violation and adding
another of the same rule and reason in the same file counts as no change. A
change that tightens `.swiftlint.yml` counts every existing violation of the
tightened rule as added, since each tree is linted with its own config.

Usage:
  swiftlint-diff.py --base base.json --base-root DIR \
                    --head head.json --head-root DIR \
                    [--renames renames.tsv] [--tree hosts/ios]
"""

import argparse
import json
import os
import re
import sys
from collections import Counter


def load(report, root):
    with open(report) as f:
        violations = json.load(f)
    for v in violations:
        v["path"] = os.path.relpath(v["file"], root)
    return violations


def load_renames(path, tree):
    """Maps each renamed file's base path to its new one, relative to the tree."""
    renames = {}
    if not path:
        return renames
    prefix = tree.rstrip("/") + "/" if tree else ""
    with open(path) as f:
        for line in f:
            fields = line.rstrip("\n").split("\t")
            if len(fields) != 3 or not fields[0].startswith("R"):
                continue
            old, new = fields[1], fields[2]
            if old.startswith(prefix) and new.startswith(prefix):
                renames[old[len(prefix):]] = new[len(prefix):]
    return renames


def key(v):
    return (v["path"], v["rule_id"], re.sub(r"\d+", "#", v["reason"]))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True)
    parser.add_argument("--base-root", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--head-root", required=True)
    parser.add_argument("--renames", default="")
    parser.add_argument("--tree", default="", help="repository path both roots stand for")
    args = parser.parse_args()

    renames = load_renames(args.renames, args.tree)
    base = load(args.base, args.base_root)
    for v in base:
        v["path"] = renames.get(v["path"], v["path"])
    head = load(args.head, args.head_root)
    surplus = Counter(key(v) for v in head) - Counter(key(v) for v in base)

    added = []
    for v in sorted(head, key=lambda v: (v["path"], v["line"] or 0)):
        if surplus[key(v)] > 0:
            surplus[key(v)] -= 1
            added.append(v)

    print(f"base={len(base)} head={len(head)} renamed={len(renames)} added={len(added)}")
    for v in added:
        path = os.path.join(args.tree, v["path"])
        message = f"{v['type']} Violation: {v['reason']} ({v['rule_id']})"
        print(f"::error file={path},line={v['line'] or 1}::{message}")

    if added:
        print(f"{len(added)} SwiftLint violation(s) added against the base.")
        return 1
    print("No SwiftLint violations added against the base.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
