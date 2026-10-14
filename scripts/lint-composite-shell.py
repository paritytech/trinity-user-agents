#!/usr/bin/env python3
"""Shellcheck the inline shell of every composite action a workflow can reach.

actionlint reads a local `action.yml` only to validate the caller's inputs, so
a composite action's `run:` blocks never reach shellcheck the way a workflow's
do. Each one is written out as its own file here and checked directly, which is
what catches an unbalanced block that would otherwise fail at run time with no
message at all.

Two prologue lines are prepended to each extracted block, so a line number in
shellcheck's output is two ahead of the same line in the `run:` block.
"""

import pathlib
import re
import subprocess
import sys
import tempfile

import yaml

# GitHub reads workflows only from the repository root, so the workflows
# vendored under hosts/android drive nothing and the actions they call are
# unreachable. Linting them would mean carrying fixes to dormant code through
# every upstream refresh. hosts/ios is not listed: nine root workflows call its
# actions, so those are live and are checked.
UNREACHABLE = ("hosts/android/.github/",)

# Expressions are interpolated before the shell ever sees them. Shellcheck
# cannot parse the braces, so each becomes a variable reference rather than a
# bare word: a bare word reads as a literal, which turns correct shell such as
# `[ ${{ inputs.n }} -gt 3 ]` into an error-severity SC2170 and hides the
# word-splitting this job exists to catch.
EXPRESSION = re.compile(r"\$\{\{.*?\}\}")
PLACEHOLDER = "gha_expr"


def repo_root():
    """Repository root, so the search does not depend on the caller's cwd."""
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    )
    return pathlib.Path(out.stdout.strip())


def shell_of(step):
    """The shell a step declares, or None if it is not one we can check.

    A composite step may spell its shell as a template, `bash -e {0}`, which
    names the same interpreter as the plain form.
    """
    declared = step.get("shell")
    if not isinstance(declared, str) or not declared.strip():
        return None
    first = declared.split()[0]
    return first if first in ("bash", "sh") else None


def blocks(root):
    """Yield (action path, step label, shell, script) for each inline run."""
    for path in sorted(root.glob("**/.github/actions/**/action.y*ml")):
        relative = path.relative_to(root).as_posix()
        if relative.startswith(UNREACHABLE):
            continue
        try:
            parsed = yaml.safe_load(path.read_text()) or {}
        except yaml.YAMLError as error:
            print(f"::error file={relative}::cannot parse: {error}", flush=True)
            raise
        for index, step in enumerate(parsed.get("runs", {}).get("steps") or []):
            if "run" not in step:
                continue
            shell = shell_of(step)
            if shell is None:
                # A run step must name a shell, and one we cannot check would
                # be skipped silently, which is the failure this script exists
                # to remove.
                print(
                    f"::error file={relative}::step {index} declares shell "
                    f"{step.get('shell')!r}, which cannot be checked",
                    flush=True,
                )
                raise SystemExit(1)
            label = step.get("name", f"step {index}")
            yield relative, label, shell, step["run"]


def main():
    root = repo_root()
    found = list(blocks(root))
    if not found:
        # Reaching zero means the layout moved, not that the tree is clean.
        print("::error::found no composite action shell to lint", flush=True)
        return 1

    failed = []
    with tempfile.TemporaryDirectory() as tmp:
        for relative, label, shell, script in found:
            print(f"linting {relative} ({label})", flush=True)
            # Named for the action and step, because shellcheck reports the
            # file it was handed and nothing else identifies the source.
            stem = relative.replace("/", "_").rsplit(".", 1)[0]
            target = pathlib.Path(tmp) / f"{stem}__{label.replace(' ', '_')}.sh"
            body, substituted = EXPRESSION.subn(f"${PLACEHOLDER}", script)
            # Declared only where it is referenced, since an assignment the
            # block never reads is itself a shellcheck finding.
            prologue = f"{PLACEHOLDER}=\n" if substituted else ""
            target.write_text(f"#!/usr/bin/env {shell}\n{prologue}{body}")
            sys.stdout.flush()
            if subprocess.run(["shellcheck", str(target)]).returncode != 0:
                failed.append(f"{relative} ({label})")

    for entry in failed:
        print(f"::error::shellcheck reported issues in {entry}", flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
