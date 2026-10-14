#!/usr/bin/env python3
"""Clone or update the TrUAPI Rust core checkout the Android build needs.

`:bindings:truapi-host` compiles the core from a checkout outside this repo, so
`settings.gradle.kts` fails configuration until `truapi.dir` resolves. This puts
the checkout on the SHA CI builds and writes the path to local.properties.

  scripts/setup-truapi.py [--dir PATH]
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = "https://github.com/paritytech/trinity-user-agents.git"
ACTION = Path(".github/actions/install/action.yaml")
# The same directory settings.gradle.kts probes to decide the checkout is real.
MARKER = Path("rust/crates/truapi")


def fail(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def git(*args, cwd=None, check=True):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=check, capture_output=True, text=True
    )


def pinned_ref(root):
    """Read the pin from the CI action so this script cannot drift from CI."""
    action = root / ACTION
    if not action.is_file():
        fail(f"{ACTION} not found; run this from the repo")
    match = re.search(
        r"^  truapi_ref:.*?^    default:\s*\"([0-9a-f]{40})\"",
        action.read_text(),
        re.S | re.M,
    )
    if not match:
        fail(f"could not read the truapi_ref default out of {ACTION}")
    return match.group(1)


def configured_dir(root):
    props = root / "local.properties"
    if not props.is_file():
        return None
    match = re.search(r"^truapi\.dir=(.*)$", props.read_text(), re.M)
    return Path(match.group(1).strip()) if match else None


def resolve_target(root, requested):
    if requested:
        return Path(requested).expanduser().resolve()
    existing = configured_dir(root)
    if existing:
        return existing.expanduser().resolve()
    return root / ".truapi"


def clone(target, ref):
    print(f"cloning {REPO} into {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    # Not --recurse-submodules: the core carries private host submodules that a
    # clone without access to them fails on, and the Android build needs none.
    git("clone", REPO, str(target))
    checkout(target, ref)


def checkout(target, ref):
    if git("rev-parse", "HEAD", cwd=target).stdout.strip().startswith(ref):
        print(f"already on {ref[:8]}")
        return
    if git("status", "--porcelain", cwd=target).stdout.strip():
        fail(
            f"{target} has uncommitted changes; commit or stash them, "
            f"then check out {ref} there yourself"
        )
    print(f"fetching {ref[:8]}")
    git("fetch", "origin", ref, cwd=target)
    git("checkout", "--detach", ref, cwd=target)


def write_local_properties(root, target):
    props = root / "local.properties"
    line = f"truapi.dir={target}"
    text = props.read_text() if props.is_file() else ""
    if re.search(r"^truapi\.dir=", text, re.M):
        text = re.sub(r"^truapi\.dir=.*$", line, text, flags=re.M)
    else:
        text = (text + "\n" if text and not text.endswith("\n") else text) + line + "\n"
    props.write_text(text)
    print(f"local.properties: {line}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dir",
        help="where to keep the checkout (default: an existing truapi.dir, else .truapi)",
    )
    args = parser.parse_args()

    root = Path(
        git("rev-parse", "--show-toplevel").stdout.strip()
    )
    ref = pinned_ref(root)
    target = resolve_target(root, args.dir)

    # A file, not a directory, when the checkout is a linked worktree.
    if (target / ".git").exists():
        checkout(target, ref)
    elif target.exists() and any(target.iterdir()):
        fail(f"{target} exists and is not a git checkout")
    else:
        clone(target, ref)

    if not (target / MARKER).is_dir():
        fail(f"{target} has no {MARKER}; that is not a trinity-user-agents checkout")

    write_local_properties(root, target)
    print("truapi core ready")


if __name__ == "__main__":
    main()
