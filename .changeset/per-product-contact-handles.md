---
"@parity/truapi-host": major
"@parity/truapi": patch
---

A contact handle from `contacts.pick` is scoped to the calling product: the same contact has a different handle in every
product, and the same one on every host of the user. A handle from another product, or one picked before this release, does not resolve.
`HostContactLookup.handle_key` is per session and product, so a host that keeps handle hashes keeps them per key.
