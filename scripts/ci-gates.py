#!/usr/bin/env python3
"""Evaluate the host registry for a change and print its gates.

`.github/hosts.json` names what every host is built from. An entry ending in
`/` is a directory and matches everything under it; any other entry is one
file. To route a new path, add it to the list below that describes it:

- `core`: an input more than one host links or bundles, such as a crate, the
  TypeScript the containers bundle, or a toolchain pin.
- `hosts.<host>.tree`: the host's own app sources.
- `hosts.<host>.full`: a shared input that runs the host's whole suite rather
  than only its core checks, such as a build script whose output no bindings
  hash would catch.
- `hosts.<host>.ci`: a workflow or action that runs the host's suite but does
  not change the app.

For each host this prints three gates as `name=true|false` lines, the format
`$GITHUB_OUTPUT` takes:

- `<host>_core`: a core input changed, so the host's core compatibility checks
  run.
- `<host>_tree`: a `tree`, `full` or `ci` path changed, so its full suite runs.
- `<host>_changed`: a `core` or `tree` path changed, so the host's app is
  different. A workflow triggered by a merge reads this to decide whether
  there is a new build.

    ci-gates.py --base <rev> [--head <rev>]           gates for base..head
    ci-gates.py --base <rev> [--head <rev>] --force   the same, with every
                                                      core and tree gate true
    ci-gates.py --all                                 every gate true
    ci-gates.py --check                               validate the registry

`--force` is for a push to main, where every suite runs so each landed commit
is checked in full, while `<host>_changed` still says whether the push changed
the app. `--all` is for a run with nothing to diff against, such as a dispatch.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = ROOT / ".github" / "hosts.json"
HOST_LISTS = ("tree", "full", "ci")


def load() -> dict:
    return json.loads(REGISTRY.read_text())


def matches(path: str, entries: list[str]) -> bool:
    return any(path.startswith(e) if e.endswith("/") else path == e for e in entries)


def changed_paths(base: str, head: str) -> list[str]:
    # A rename lists both its source and destination, and -z keeps paths
    # unquoted, so every path that moved is compared as git stores it.
    out = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "-z", base, head],
        cwd=ROOT, check=True, capture_output=True,
    ).stdout
    return [p for p in out.decode("utf-8", errors="surrogateescape").split("\0") if p]


def gates(registry: dict, paths: list[str] | None, force: bool = False) -> dict[str, bool]:
    every = paths is None

    def hit(entries: list[str]) -> bool:
        return every or any(matches(p, entries) for p in paths)

    result = {}
    core = hit(registry["core"])
    for host, spec in registry["hosts"].items():
        tree = hit(spec["tree"])
        suite = tree or hit(spec["full"]) or hit(spec["ci"])
        result[f"{host}_core"] = core or force
        result[f"{host}_tree"] = suite or force
        result[f"{host}_changed"] = core or tree
    return result


def is_gitlink(entry: str) -> bool:
    out = subprocess.run(
        ["git", "ls-files", "-s", "--", entry],
        cwd=ROOT, capture_output=True, text=True,
    ).stdout
    return out.startswith("160000 ")


def check(registry: dict) -> list[str]:
    """Every entry exists, no list repeats an entry, and a host's own tree is
    not also shared core."""
    problems = []
    lists = {"core": registry["core"]}
    for host, spec in registry["hosts"].items():
        for name in HOST_LISTS:
            if name not in spec:
                problems.append(f"hosts.{host} has no {name} list")
            lists[f"hosts.{host}.{name}"] = spec.get(name, [])
    for where, entries in lists.items():
        if len(set(entries)) != len(entries):
            problems.append(f"{where} lists an entry more than once")
        for entry in entries:
            target = ROOT / entry.rstrip("/")
            if entry.endswith("/") and not target.is_dir():
                problems.append(f"{where}: {entry} is not a directory")
            elif not entry.endswith("/") and not (target.is_file() or is_gitlink(entry)):
                problems.append(f"{where}: {entry} is not a file")
    for host, spec in registry["hosts"].items():
        for entry in spec.get("tree", []):
            if entry in registry["core"]:
                problems.append(f"hosts.{host}.tree: {entry} is shared core, not the host's own")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--base", help="revision to diff from")
    mode.add_argument("--all", action="store_true", help="every gate true")
    mode.add_argument("--check", action="store_true", help="validate the registry")
    parser.add_argument("--head", default="HEAD", help="revision to diff to")
    parser.add_argument("--force", action="store_true", help="force every core and tree gate")
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
    for name, value in gates(registry, paths, force=args.force).items():
        print(f"{name}={'true' if value else 'false'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
