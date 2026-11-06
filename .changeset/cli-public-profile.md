---
"@parity/truapi": minor
"@parity/truapi-host": patch
---

The signing-host CLI supports `/profile`, `/profile <username>`, and `/profile upload <image>`. Uploads store PNG, JPEG, GIF, or WebP avatars on Bulletin and publish their CID in the active identity's dotNS profile, preserving unknown entries. Host profile uploads share the core's Bulletin allowance and submission handling with product calls.
