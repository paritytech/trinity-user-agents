#!/usr/bin/env python3
"""Evaluate the host registry for a change and print its gates.

`.github/hosts.json` names what every host is built from: `core`, the shared
inputs each host links or bundles, and each host's own `tree`. An entry ending
in `/` is a directory and matches everything under it; any other entry is one
file.

For each host this prints three gates as `name=true|false` lines, the format
`$GITHUB_OUTPUT` takes:

- `<host>_core`: a core input changed, so the host's core compatibility checks
  run.
- `<host>_tree`: one of the host's own inputs changed, so its full suite runs.
- `<host>_changed`: either changed, so the host's app is different. A workflow
  triggered by a merge reads this to decide whether there is a new build.

    ci-gates.py --base <rev> [--head <rev>]   gates for the diff base...head
    ci-gates.py --all                         every gate true
    ci-gates.py --check                       validate the registry

`--all` is for a run with no base to diff against, such as a push to main or a
dispatch, where every gate runs so each landed commit is checked in full.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = ROOT / ".github" / "hosts.json"


def load() -> dict:
    return json.loads(REGISTRY.read_text())


def matches(path: str, entries: list[str]) -> bool:
    return any(path.startswith(e) if e.endswith("/") else path == e for e in entries)


def changed_paths(base: str, head: str) -> list[str]:
    out = subprocess.run(
        ["git", "diff", "--name-only", base, head],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def gates(registry: dict, paths: list[str] | None) -> dict[str, bool]:
    result = {}
    core = registry["core"]
    core_changed = paths is None or any(matches(p, core) for p in paths)
    for host, spec in registry["hosts"].items():
        tree_changed = paths is None or any(matches(p, spec["tree"]) for p in paths)
        result[f"{host}_core"] = core_changed
        result[f"{host}_tree"] = tree_changed
        result[f"{host}_changed"] = core_changed or tree_changed
    return result


def check(registry: dict) -> list[str]:
    """Every entry names a path that exists, and no entry is listed twice."""
    problems = []
    seen = {}
    lists = {"core": registry["core"]}
    lists.update({f"hosts.{h}.tree": s["tree"] for h, s in registry["hosts"].items()})
    for where, entries in lists.items():
        for entry in entries:
            target = ROOT / entry.rstrip("/")
            if entry.endswith("/") and not target.is_dir():
                problems.append(f"{where}: {entry} is not a directory")
            elif not entry.endswith("/") and not target.is_file():
                problems.append(f"{where}: {entry} is not a file")
            if entry in seen:
                problems.append(f"{where}: {entry} is already listed in {seen[entry]}")
            seen.setdefault(entry, where)
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--base", help="revision to diff against")
    mode.add_argument("--all", action="store_true", help="every gate true")
    mode.add_argument("--check", action="store_true", help="validate the registry")
    parser.add_argument("--head", default="HEAD", help="revision to diff to")
    args = parser.parse_args()

    registry = load()
    if args.check:
        problems = check(registry)
        for problem in problems:
            print(f"hosts.json: {problem}", file=sys.stderr)
        if not problems:
            print(f"hosts.json names {len(registry['hosts'])} hosts and every entry exists")
        return 1 if problems else 0

    paths = None if args.all else changed_paths(args.base, args.head)
    for name, value in gates(registry, paths).items():
        print(f"{name}={'true' if value else 'false'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
