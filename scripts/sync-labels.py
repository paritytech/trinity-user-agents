#!/usr/bin/env python3
"""Make the repository's labels match .github/labels.yml.

Usage:
    scripts/sync-labels.py [--apply] [--repo OWNER/NAME]

Without --apply it only prints the plan. Reads and writes go through the `gh`
CLI, so GH_TOKEN or a logged-in `gh` decides what it may do.

A label is renamed rather than recreated whenever one of its `previously` names
exists, because a rename keeps the label on every issue and pull request that
carries it. A second previous name is merged instead: its items get the new
label, then the old one is deleted. Labels the file does not mention are
reported and left alone; only `retired` ones are deleted.
"""

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import urllib.parse

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
CONFIG = ROOT / ".github" / "labels.yml"
TARGETS = {"prs", "issues", "both"}


def gh(*args: str, method: str = "GET", fields: dict | None = None) -> object:
    cmd = ["gh", "api", "-X", method, *args]
    for key, value in (fields or {}).items():
        if isinstance(value, list):
            for item in value:
                cmd += ["-f", f"{key}[]={item}"]
        else:
            cmd += ["-f", f"{key}={value}"]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True).stdout
    return json.loads(out) if out.strip() else None


def gh_paginate(path: str) -> list:
    out = subprocess.run(
        ["gh", "api", "--paginate", "--slurp", path],
        check=True, capture_output=True, text=True,
    ).stdout
    return [item for page in json.loads(out) for item in page]


def quoted(name: str) -> str:
    return urllib.parse.quote(name, safe="")


def load() -> dict:
    data = yaml.safe_load(CONFIG.read_text())
    errors = []
    seen = {}

    def claim(name: str, where: str) -> None:
        key = name.casefold()
        if key in seen:
            errors.append(f"{name!r} appears as {seen[key]} and as {where}")
        seen[key] = where

    for label in data.get("labels", []):
        name = label.get("name", "")
        claim(name, "a label")
        if not re.fullmatch(r"[0-9a-fA-F]{6}", str(label.get("color", ""))):
            errors.append(f"{name!r} needs a six digit hex color")
        if not label.get("description"):
            errors.append(f"{name!r} needs a description")
        elif len(label["description"]) > 100:
            errors.append(f"{name!r} has a description over GitHub's 100 characters")
        if label.get("target") not in TARGETS:
            errors.append(f"{name!r} needs a target of prs, issues or both")
        if not label.get("addedBy"):
            errors.append(f"{name!r} needs an addedBy")
        for old in label.get("previously", []):
            claim(old, f"a previous name of {name!r}")
    for section in ("retired", "pending"):
        for entry in data.get(section, []):
            claim(entry["name"], f"{section}")
            if not entry.get("reason"):
                errors.append(f"{entry['name']!r} in {section} needs a reason")

    if errors:
        for error in errors:
            print(f"::error file=.github/labels.yml::{error}")
        sys.exit(1)
    return data


def plan(data: dict, existing: dict) -> list[tuple]:
    """Steps in the order they must run. Each is (kind, detail...)."""
    steps = []
    remaining = dict(existing)  # casefolded name -> label, as it will be

    for label in data["labels"]:
        name, color, description = label["name"], label["color"].lower(), label["description"]
        current = remaining.get(name.casefold())
        olds = [remaining[o.casefold()] for o in label.get("previously", []) if o.casefold() in remaining]

        if current is None and olds:
            first = olds.pop(0)
            steps.append(("rename", first["name"], name, color, description))
            remaining.pop(first["name"].casefold())
            current = {"name": name, "color": color, "description": description}
            remaining[name.casefold()] = current
        elif current is None:
            steps.append(("create", name, color, description))
            current = {"name": name, "color": color, "description": description}
            remaining[name.casefold()] = current
        elif (current["color"].lower(), current.get("description") or "") != (color, description) \
                or current["name"] != name:
            steps.append(("update", current["name"], name, color, description))

        for old in olds:
            steps.append(("merge", old["name"], name))
            remaining.pop(old["name"].casefold())

    for entry in data.get("retired", []):
        if entry["name"].casefold() in remaining:
            steps.append(("delete", remaining.pop(entry["name"].casefold())["name"], entry["reason"]))

    known = {l["name"].casefold() for l in data["labels"]}
    known |= {e["name"].casefold() for e in data.get("pending", [])}
    for key, label in sorted(remaining.items()):
        if key not in known:
            steps.append(("unmanaged", label["name"]))
    return steps


def describe(step: tuple) -> str:
    kind = step[0]
    if kind == "create":
        return f"create `{step[1]}`"
    if kind == "rename":
        return f"rename `{step[1]}` to `{step[2]}`, keeping it wherever it is applied"
    if kind == "update":
        return f"update `{step[2]}`'s color or description"
    if kind == "merge":
        return f"move everything labelled `{step[1]}` to `{step[2]}`, then delete `{step[1]}`"
    if kind == "delete":
        return f"delete `{step[1]}`: {step[2]}"
    return f"leave `{step[1]}`: not in labels.yml"


def apply(repo: str, step: tuple) -> None:
    kind = step[0]
    if kind == "create":
        gh(f"repos/{repo}/labels", method="POST",
           fields={"name": step[1], "color": step[2], "description": step[3]})
    elif kind in ("rename", "update"):
        gh(f"repos/{repo}/labels/{quoted(step[1])}", method="PATCH",
           fields={"new_name": step[2], "color": step[3], "description": step[4]})
    elif kind == "merge":
        old, new = step[1], step[2]
        items = gh_paginate(f"repos/{repo}/issues?state=all&per_page=100&labels={quoted(old)}")
        for item in items:
            gh(f"repos/{repo}/issues/{item['number']}/labels", method="POST", fields={"labels": [new]})
        gh(f"repos/{repo}/labels/{quoted(old)}", method="DELETE")
    elif kind == "delete":
        gh(f"repos/{repo}/labels/{quoted(step[1])}", method="DELETE")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", action="store_true", help="make the changes rather than print them")
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "paritytech/trinity-user-agents"))
    args = parser.parse_args()

    data = load()
    existing = {l["name"].casefold(): l for l in gh_paginate(f"repos/{args.repo}/labels?per_page=100")}
    steps = plan(data, existing)
    changes = [s for s in steps if s[0] != "unmanaged"]

    lines = [f"### Labels: {len(changes)} change{'s' if len(changes) != 1 else ''}", ""]
    lines += [f"- {describe(s)}" for s in steps] or ["- nothing to do"]
    report = "\n".join(lines)
    print(report)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a") as fh:
            fh.write(report + "\n")

    if args.apply:
        for step in changes:
            apply(args.repo, step)
            print(f"done: {describe(step)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
