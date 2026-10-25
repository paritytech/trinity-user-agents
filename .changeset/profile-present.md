---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `profile` service. `profile.present({ reference })` asks the host to show a referenced profile in host-owned
UI; the host resolves, decrypts and renders it, and nothing but acceptance returns to the product. Hosts opt in with the
optional `profile` callbacks (`ProfilePlatform`); a host that supplies none answers `Unsupported`.

Keep the optional profile bindings available under the consolidated native
`runtime` feature, and use the shared host clock for disclosure revisions.
