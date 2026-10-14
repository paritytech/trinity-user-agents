#!/usr/bin/env python3
"""
Simple script for reading versions from Xcode project file.
"""

import argparse
import os
import re
import sys


def read_versions(pbxproj_path, config_name=None):
    """
    Read current MARKETING_VERSION and CURRENT_PROJECT_VERSION from Xcode project.

    Args:
        pbxproj_path: Path to project.pbxproj file
        config_name: Optional configuration name to read from specific build config (e.g., 'Release', 'DevCI')

    Returns:
        dict with 'marketing_version', 'build_number', 'major', 'minor'
    """
    if not os.path.isfile(pbxproj_path):
        raise FileNotFoundError(f"Xcode project file not found at {pbxproj_path}")

    with open(pbxproj_path, "r", encoding="utf-8", errors="ignore") as f:
        content = f.read()

    if config_name:
        # Read from specific configuration block by name
        # Pattern: <config_id> /* <config_name> */ = { ... };
        # A configuration name appears once per shippable target, once per test target and
        # once at project level. Only the shippable targets take their bundle id from
        # $(BUNDLE) (the xcconfig-driven identity), and only they carry the version we
        # ship — test targets pin an unrelated MARKETING_VERSION. Select on that marker
        # rather than on block order, which the pbxproj is free to change.
        config_pattern = (
            rf"\w{{24}}\s*/\*\s*{re.escape(config_name)}\s*\*/\s*=\s*\{{[^}}]*?\}}"
        )
        blocks = re.findall(config_pattern, content, re.DOTALL)
        if not blocks:
            raise ValueError(f"Configuration '{config_name}' not found")

        shippable = [
            block
            for block in blocks
            if 'PRODUCT_BUNDLE_IDENTIFIER = "$(BUNDLE)"' in block
            and "MARKETING_VERSION" in block
        ]
        if not shippable:
            raise ValueError(
                f"Configuration '{config_name}' has no target block with both "
                'PRODUCT_BUNDLE_IDENTIFIER = "$(BUNDLE)" and MARKETING_VERSION'
            )

        search_text = shippable[0]
    else:
        # Read from anywhere in the file (take first occurrence)
        search_text = content

    # Find MARKETING_VERSION (supports X.Y or X.Y.Z)
    m_ver = re.search(
        r"MARKETING_VERSION\s*=\s*([0-9]+)\.([0-9]+)(?:\.([0-9]+))?;", search_text
    )
    if not m_ver:
        raise ValueError(
            "Failed to find MARKETING_VERSION in supported format (X.Y or X.Y.Z)"
        )

    major, minor, patch = m_ver.group(1), m_ver.group(2), m_ver.group(3)
    has_patch = patch is not None
    marketing_version = f"{major}.{minor}.{patch}" if has_patch else f"{major}.{minor}"

    # Find CURRENT_PROJECT_VERSION (build number)
    builds = [
        int(b) for b in re.findall(r"CURRENT_PROJECT_VERSION\s*=\s*(\d+);", search_text)
    ]
    if not builds:
        raise ValueError("Failed to find CURRENT_PROJECT_VERSION")

    build_number = max(builds) if config_name is None else builds[0]

    return {
        "marketing_version": marketing_version,
        "build_number": build_number,
        "major": int(major),
        "minor": int(minor),
        "patch": int(patch) if patch is not None else 0,
        "has_patch": has_patch,
    }


def main():
    parser = argparse.ArgumentParser(description="Read versions from Xcode project")
    parser.add_argument("pbxproj", help="Path to project.pbxproj file")
    parser.add_argument(
        "--config-name",
        help="Configuration name for reading from specific build config (e.g., 'Release', 'DevCI')",
    )
    parser.add_argument(
        "--output-format",
        choices=["env", "json"],
        default="env",
        help="Output format (default: env)",
    )

    args = parser.parse_args()

    try:
        versions = read_versions(args.pbxproj, args.config_name)

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
