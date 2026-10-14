#!/usr/bin/env python3
"""
Simple script for reading versions from Android Versions.kt file.
"""

import argparse
import os
import re
import sys


def read_versions(versions_kt_path):
    """
    Read current DefaultVersionName and DefaultVersionCode from Versions.kt file.

    Args:
        versions_kt_path: Path to Versions.kt file

    Returns:
        dict with 'marketing_version', 'build_number', 'major', 'minor', 'patch'
    """
    if not os.path.isfile(versions_kt_path):
        raise FileNotFoundError(f"Versions.kt file not found at {versions_kt_path}")

    with open(versions_kt_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Find DefaultVersionName (supports X.Y or X.Y.Z format)
    version_match = re.search(
        r'private\s+const\s+val\s+DefaultVersionName\s*=\s*"([0-9]+)\.([0-9]+)(?:\.([0-9]+))?"',
        content,
    )
    if not version_match:
        raise ValueError(
            "Failed to find DefaultVersionName in supported format (X.Y or X.Y.Z) in Versions.kt"
        )

    major, minor, patch = (
        version_match.group(1),
        version_match.group(2),
        version_match.group(3),
    )
    has_patch = patch is not None
    marketing_version = f"{major}.{minor}.{patch}" if has_patch else f"{major}.{minor}"

    # Find DefaultVersionCode (build number)
    build_match = re.search(
        r"private\s+const\s+val\s+DefaultVersionCode\s*=\s*(\d+)", content
    )
    if not build_match:
        raise ValueError("Failed to find DefaultVersionCode in Versions.kt")

    build_number = int(build_match.group(1))

    return {
        "marketing_version": marketing_version,
        "build_number": build_number,
        "major": int(major),
        "minor": int(minor),
        "patch": int(patch) if patch is not None else 0,
        "has_patch": has_patch,
    }


def main():
    parser = argparse.ArgumentParser(
        description="Read versions from Android Versions.kt file"
    )
    parser.add_argument(
        "versions_kt",
        nargs="?",
        default="build-logic/convention/src/main/kotlin/Versions.kt",
        help="Path to Versions.kt file (default: build-logic/convention/src/main/kotlin/Versions.kt)",
    )
    parser.add_argument(
        "--output-format",
        choices=["env", "json"],
        default="env",
        help="Output format (default: env)",
    )

    args = parser.parse_args()

    try:
        versions = read_versions(args.versions_kt)

        if args.output_format == "json":
            import json

            print(json.dumps(versions))
        else:  # env format for GitHub Actions
            print(f"marketing_version={versions['marketing_version']}")
            print(f"build_number={versions['build_number']}")
            print(f"major={versions['major']}")
            print(f"minor={versions['minor']}")
            print(f"patch={versions['patch']}")
            print(f"has_patch={'true' if versions['has_patch'] else 'false'}")

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
