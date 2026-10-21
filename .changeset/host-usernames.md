---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `Usernames` service (wire trait 27): `usernames.search({ prefix })` gives at most 20 usernames that start with the prefix. A native host installs a directory with `NativeTrUApiHostRuntime.set_usernames_callbacks`; a host without one answers `Unsupported`.
