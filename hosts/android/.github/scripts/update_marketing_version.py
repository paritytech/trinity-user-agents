#!/usr/bin/env python3
"""
Simple script for updating DefaultVersionName (marketing version) in Android Versions.kt file.
"""

import argparse
import os
import re
import sys


def update_marketing_version(versions_kt_path, new_version):
    """
    Update DefaultVersionName in Versions.kt file.

    Args:
        versions_kt_path: Path to Versions.kt file
        new_version: New DefaultVersionName (X.Y.Z format)

    Returns:
        bool indicating if the version was updated
    """
    if not os.path.isfile(versions_kt_path):
        raise FileNotFoundError(f"Versions.kt file not found at {versions_kt_path}")

    # Validate version format (supports X.Y or X.Y.Z)
    if not re.match(r"^\d+\.\d+(?:\.\d+)?$", new_version):
        raise ValueError(
            f"Invalid version format: {new_version}. Must be X.Y or X.Y.Z format"
        )

    with open(versions_kt_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Find current version (supports X.Y or X.Y.Z)
    current_match = re.search(
        r'private\s+const\s+val\s+DefaultVersionName\s*=\s*"([0-9]+\.[0-9]+(?:\.[0-9]+)?)"',
        content,
    )

    if not current_match:
        raise ValueError("Could not find DefaultVersionName in Versions.kt")

    current_version = current_match.group(1)

    if current_version == new_version:
        print(f"DefaultVersionName is already {new_version}")
        return False

    # Update version (supports X.Y or X.Y.Z)
    new_content = re.sub(
        r'(private\s+const\s+val\s+DefaultVersionName\s*=\s*)"[0-9]+\.[0-9]+(?:\.[0-9]+)?"',
        rf'\g<1>"{new_version}"',
        content,
    )

    with open(versions_kt_path, "w", encoding="utf-8") as f:
        f.write(new_content)

    print(f"Updated DefaultVersionName: {current_version} -> {new_version}")
    return True


def main():
    parser = argparse.ArgumentParser(
        description="Update DefaultVersionName in Android Versions.kt file"
    )
    parser.add_argument("version", help="New DefaultVersionName (X.Y.Z format)")
    parser.add_argument(
        "versions_kt",
        nargs="?",
        default="build-logic/convention/src/main/kotlin/Versions.kt",
        help="Path to Versions.kt file (default: build-logic/convention/src/main/kotlin/Versions.kt)",
    )

    args = parser.parse_args()

    try:
        updated = update_marketing_version(args.versions_kt, args.version)

        if not updated:
            sys.exit(0)  # No changes needed

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
