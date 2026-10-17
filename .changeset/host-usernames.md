---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `Usernames` service (wire trait 27): `usernames.search({ prefix })` gives at most 20 usernames that start with the prefix, and `usernames.addContact({ username })` asks the user to add the owner of the username to the contacts of the app (`Added`, `AlreadyContact`, `Dismissed` or `NotFound`). An add follows the press rule of `Share`. A native host installs a directory with `NativeTrUApiHostRuntime.set_usernames_callbacks`; a host without one answers `Unsupported`.
