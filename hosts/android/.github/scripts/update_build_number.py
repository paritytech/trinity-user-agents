#!/usr/bin/env python3
"""
Simple script for updating DefaultVersionCode (build number) in Android Versions.kt file.
Can either increment the current build number or set it to a specific value.
"""

import argparse
import os
import re
import sys


def update_build_number(versions_kt_path, build_number=None):
    """
    Update DefaultVersionCode in Versions.kt file.

    Args:
        versions_kt_path: Path to Versions.kt file
        build_number: If provided, set to this value. If None, increment current value.

    Returns:
        Tuple of (success: bool, new_build_number: int)
    """
    if not os.path.isfile(versions_kt_path):
        raise FileNotFoundError(f"Versions.kt file not found at {versions_kt_path}")

    with open(versions_kt_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Find current build number
    build_match = re.search(
        r"private\s+const\s+val\s+DefaultVersionCode\s*=\s*(\d+)", content
    )

    if not build_match:
        raise ValueError("Could not find DefaultVersionCode in Versions.kt")

    current_build = int(build_match.group(1))

    if build_number is not None:
        # Set to specific value
        new_build = build_number
        action = "Set"
    else:
        # Increment current value
        new_build = current_build + 1
        action = "Incremented"

    # Update build number
    new_content = re.sub(
        r"(private\s+const\s+val\s+DefaultVersionCode\s*=\s*)\d+",
        rf"\g<1>{new_build}",
        content,
    )

    with open(versions_kt_path, "w", encoding="utf-8") as f:
        f.write(new_content)

    print(f"{action} DefaultVersionCode: {current_build} -> {new_build}")
    return True, new_build


def main():
    parser = argparse.ArgumentParser(
        description="Update DefaultVersionCode in Android Versions.kt file"
    )
    parser.add_argument(
        "versions_kt",
        nargs="?",
        default="build-logic/convention/src/main/kotlin/Versions.kt",
        help="Path to Versions.kt file (default: build-logic/convention/src/main/kotlin/Versions.kt)",
    )
    parser.add_argument(
        "--build-number",
        type=int,
        help="Set build number to this value (if not provided, will increment current value)",
    )
    parser.add_argument(
        "--output-github",
        action="store_true",
        help="Output in GitHub Actions format to GITHUB_OUTPUT",
    )

    args = parser.parse_args()

    try:
        success, new_build = update_build_number(args.versions_kt, args.build_number)

        if args.output_github and os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as f:
                f.write(f"new_build={new_build}\n")
                f.write(f"build_updated=true\n")

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
