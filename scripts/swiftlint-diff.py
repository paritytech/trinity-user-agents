#!/usr/bin/env python3
"""Fail when a change adds a SwiftLint violation that its base does not have.

Takes two `swiftlint lint --reporter json` reports, one from the base tree and
one from the change, and the directory each was linted from. A violation is
keyed on its file, rule and reason with digit runs collapsed, so that a line
shift or a new count in the reason ("currently contains 508") does not read as
a new violation. Multiplicity is kept, so a second instance of an existing
violation in the same file still counts as added.

Usage:
  swiftlint-diff.py --base base.json --base-root DIR \
                    --head head.json --head-root DIR [--annotate-prefix hosts/ios]
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


def key(v):
    return (v["path"], v["rule_id"], re.sub(r"\d+", "#", v["reason"]))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True)
    parser.add_argument("--base-root", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--head-root", required=True)
    parser.add_argument("--annotate-prefix", default="")
    args = parser.parse_args()

    base = load(args.base, args.base_root)
    head = load(args.head, args.head_root)
    surplus = Counter(key(v) for v in head) - Counter(key(v) for v in base)

    added = []
    for v in sorted(head, key=lambda v: (v["path"], v["line"] or 0)):
        if surplus[key(v)] > 0:
            surplus[key(v)] -= 1
            added.append(v)

    print(f"base={len(base)} head={len(head)} added={len(added)}")
    for v in added:
        path = os.path.join(args.annotate_prefix, v["path"])
        message = f"{v['type']} Violation: {v['reason']} ({v['rule_id']})"
        print(f"::error file={path},line={v['line'] or 1}::{message}")

    if added:
        print(f"{len(added)} SwiftLint violation(s) added against the base.")
        return 1
    print("No SwiftLint violations added against the base.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
