---
"@parity/truapi": minor
---

`truapi-host` keeps each stack's sessions, pairings, and core and product storage in a directory of its own under
`<base-path>/<version>`. A built-in network keeps the directory named after it. A stack given with `--stack` that is not
a built-in network's gets `stack-<tag>`, where `<tag>` is the first 16 hex digits of the SHA-256 of the stack's id, so a
fork no longer starts with its source's session, permissions and product data, and a descriptor cannot pick another
stack's directory by the `id` it declares. State such a stack stored under its old directory is not read; pair again
there. The account store is still shared, by product namespace.
