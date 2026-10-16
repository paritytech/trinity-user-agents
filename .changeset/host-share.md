---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `Share` service (wire trait 26): `share.share({ text })` opens the host's own share sheet, and the product learns only whether the user shared the text (`Shared`) or closed the sheet (`Dismissed`). A Worker may share only within 5 seconds of a user press on something the host drew for it: a renderer action, or a button, menu entry, quick reply or command of one of its rooms. A native host installs a sheet with `NativeTrUApiHostRuntime.set_share_callbacks`; a host without one answers `Unsupported`.
