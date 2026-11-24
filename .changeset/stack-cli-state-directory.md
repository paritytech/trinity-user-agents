---
"@parity/truapi": minor
---

`truapi-host` keeps everything it stores for a stack under one name: the stack's directory under
`<base-path>/<version>`, and its network in an account store. A built-in network keeps its name. A stack given with
`--stack` that is not a built-in network's gets `stack-<tag>`, where `<tag>` is the first 32 hex digits of the SHA-256
of the stack's id, so a fork no longer reads its source's sessions, accounts, pairings, or core and product storage, and
a descriptor cannot pick another stack's by the `id` it declares. `network-check` prints the name as `state_directory`.

A stack given with `--stack` starts empty after the update: provision or import its signer and pair again. What it
stored before is not read. It stays, mnemonics included, in `<base-path>/<version>/<id>`, where `<id>` is the
descriptor's `id` without a trailing `-fork`, which is also where a built-in network of that name keeps its state.
