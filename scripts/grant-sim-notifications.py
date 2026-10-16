#!/usr/bin/env python3
# Copyright 2026 Parity Technologies (UK) Ltd.
# SPDX-License-Identifier: AGPL-3.0-only
"""Give an app on an iOS simulator the notification permission, for tests.

The harness cannot tap the system prompt of a simulator, and `simctl privacy`
has no notification service. This writes the notification settings of the app
into the BulletinBoard store of the simulator, as an "Allow" tap would: alerts
as banners, in Notification Center and on the lock screen. Run it while the
simulator is shut down; the settings take effect at the next boot.

Usage: grant-sim-notifications.py <simulator udid> <bundle id> [display name]
       grant-sim-notifications.py --ready <simulator udid>
         exits 0 when the store of a booted simulator has its first sections
"""

import plistlib
import sys
from pathlib import Path

AUTHORIZED = 2
ENABLED = 2
BANNER = 1


def section_store(udid: str) -> Path:
    return (
        Path.home()
        / "Library/Developer/CoreSimulator/Devices"
        / udid
        / "data/Library/BulletinBoard/VersionedSectionInfo.plist"
    )


def granted_section(template: bytes, bundle: str, name: str) -> bytes:
    """A copy of the archived section info `template` for `bundle`, authorized."""
    archive = plistlib.loads(template)
    objects = archive["$objects"]
    info = objects[1]
    objects[info["sectionID"].data] = bundle
    objects[info["displayName"].data] = name
    objects[info["appName"].data] = name
    settings = objects[info["sectionInfoSettings"].data]
    settings.update(
        authorizationStatus=AUTHORIZED,
        alertType=BANNER,
        lockScreenSetting=ENABLED,
        notificationCenterSetting=ENABLED,
        showsInLockScreen=True,
        showsInNotificationCenter=True,
    )
    return plistlib.dumps(archive, fmt=plistlib.FMT_BINARY)


def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "--ready":
        store = section_store(sys.argv[2])
        try:
            with store.open("rb") as file:
                ready = bool(plistlib.load(file).get("sectionInfo"))
        except (OSError, plistlib.InvalidFileException):
            ready = False
        sys.exit(0 if ready else 1)
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    udid, bundle = sys.argv[1], sys.argv[2]
    name = sys.argv[3] if len(sys.argv) == 4 else bundle
    store = section_store(udid)
    with store.open("rb") as file:
        sections = plistlib.load(file)
    infos = sections["sectionInfo"]
    template = next(
        (blob for key, blob in sorted(infos.items()) if key.startswith("com.apple.")),
        None,
    )
    if template is None:
        sys.exit(f"{store} has no section to copy; boot the simulator once first")
    infos[bundle] = granted_section(template, bundle, name)
    with store.open("wb") as file:
        plistlib.dump(sections, file, fmt=plistlib.FMT_BINARY)
    print(f"notifications allowed for {bundle} on {udid}")


if __name__ == "__main__":
    main()
