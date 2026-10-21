---
"@parity/truapi": minor
---

The CLI gains `truapi-host stack`, for Trinity stack descriptors: `keygen` writes a signing key, `publish` renders a
model to `.well-known/trinity-stack.json` and keeps `sequence` in a state file, `verify` checks a link, URL or file and
reports whether it is signed, stale or expired, `bundle` writes the signed copy a host build ships and refuses an
expired one or one that would lower `sequence`, and `link` prints a pointer's link and QR code.
