#!/usr/bin/env python3
#
# normalize_warnings.py
# Read `xcrun xcresulttool get build-results --format json` output on stdin and
# emit a normalized, repo-relative warnings list on stdout:
#
#   {"count": N, "warnings": [{"path": ..., "line": ..., "message": ...}, ...]}
#
# Usage:
#   xcrun xcresulttool get build-results --path Build.xcresult --format json \
#     | normalize_warnings.py <repo_root> > warnings.json
#
# Schema (Xcode 16+ build-results JSON, stable through Xcode 26):
#   top-level `warnings[]`, each with `message`, `issueType`, `sourceURL`.
#   `sourceURL` is a file URL with a fragment carrying the location, e.g.
#   file:///abs/foo.swift#StartingLineNumber=7&StartingColumnNumber=82&...
#   There are NO separate path/line fields; they are parsed out of `sourceURL`.
#
# Filtering: keep only warnings whose source file lives inside <repo_root>, and
# drop cloned 3rd-party SPM checkouts (source_packages/). Warnings with no
# sourceURL (CoreData model, build-phase warnings) are kept with an empty path
# and a null line — keyed by message so they still ratchet.
import json
import sys
from urllib.parse import unquote

# Relative-path prefixes to drop even when located under the repo root:
# cloned remote Swift packages (gym cloned_source_packages_path: 'source_packages')
# and any stray DerivedData / .build artifacts.
DROP_PREFIXES = ("source_packages/", ".build/", "DerivedData/")


def parse_source_url(source_url, repo_root):
    """Return (relative_path, line) or (None, None) when out of repo scope."""
    if not source_url:
        return ("", None)  # synthetic: kept, keyed by message

    frag = ""
    url = source_url
    if "#" in source_url:
        url, frag = source_url.split("#", 1)

    path = url
    if path.startswith("file://"):
        path = path[len("file://"):]
    path = unquote(path)

    line = None
    for part in frag.split("&"):
        if part.startswith("StartingLineNumber="):
            try:
                # xcresult line numbers are 0-based; present as 1-based.
                line = int(part.split("=", 1)[1]) + 1
            except ValueError:
                line = None
            break

    if not path.startswith(repo_root):
        return (None, None)  # 3rd-party / system / out of tree

    rel = path[len(repo_root):].lstrip("/")
    for pfx in DROP_PREFIXES:
        if rel.startswith(pfx):
            return (None, None)
    return (rel, line)


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("usage: normalize_warnings.py <repo_root>\n")
        sys.exit(2)
    repo_root = sys.argv[1].rstrip("/")

    raw = sys.stdin.read()
    data = json.loads(raw) if raw.strip() else {}

    seen = set()
    out = []
    for w in (data.get("warnings") or []):
        message = (w.get("message") or "").strip()
        path, line = parse_source_url(w.get("sourceURL"), repo_root)
        if path is None:
            continue  # dropped (out of repo / 3rd-party)
        key = (path, line, message)
        if key in seen:
            continue
        seen.add(key)
        out.append({"path": path, "line": line, "message": message})

    out.sort(key=lambda e: (e["path"], e["line"] if e["line"] is not None else -1, e["message"]))
    json.dump({"count": len(out), "warnings": out}, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
