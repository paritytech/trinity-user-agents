#!/usr/bin/env python3
"""
Appends extra Swift compiler flags to OTHER_SWIFT_FLAGS in an Xcode xcconfig file.

Each flag is wrapped in quotes before appending, e.g.:
  -DFLAG1 -DFLAG2  ->  "-DFLAG1" "-DFLAG2"

Usage:
  python3 add_swift_flags.py <xcconfig_path> -DFLAG1 -DFLAG2
"""

import argparse
import re
import sys


def add_swift_flags(xcconfig_path, flags):
    """
    Append extra Swift flags to OTHER_SWIFT_FLAGS line in xcconfig file.

    Args:
        xcconfig_path: Path to the .xcconfig file
        flags: Space-separated flags string (e.g. "-DFLAG1 -DFLAG2")
    """
    with open(xcconfig_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Convert space-separated flags to quoted format: -DFLAG1 -DFLAG2 -> "-DFLAG1" "-DFLAG2"
    quoted_flags = re.sub(r"(-D\S+)", r'"\1"', flags)

    pattern = r"(OTHER_SWIFT_FLAGS\s*=\s*.*)"
    match = re.search(pattern, content)

    if not match:
        raise ValueError(
            f"OTHER_SWIFT_FLAGS not found in {xcconfig_path}"
        )

    old_line = match.group(1)
    new_line = f"{old_line} {quoted_flags}"

    new_content = content.replace(old_line, new_line)

    with open(xcconfig_path, "w", encoding="utf-8") as f:
        f.write(new_content)

    print(f"Added Swift flags: {quoted_flags}")
    print(f"Updated line: {new_line}")


def main():
    parser = argparse.ArgumentParser(
        description="Add extra Swift flags to xcconfig file"
    )
    parser.add_argument("xcconfig", help="Path to the .xcconfig file")

    args, remaining = parser.parse_known_args()

    if not remaining:
        parser.error("flags are required (e.g. -- -DFLAG1 -DFLAG2)")

    flags = " ".join(remaining)

    try:
        add_swift_flags(args.xcconfig, flags)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
