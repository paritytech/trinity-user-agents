---
"@parity/truapi": minor
---

`truapi-host` picked a stack's state directory by the `id` its descriptor declares. A fork declares its source's id, so
it started with the source's sessions, accounts, pairings and stored data. So did any stack that named itself like a
built-in network.

Now the CLI keeps everything it stores for a stack under a name made from the stack's id. A built-in network keeps its
directory. Any other stack gets `stack-<tag>`, where `<tag>` is the first 32 hex digits of the SHA-256 of the id.
`network-check` prints the name as `state_directory`.

Breaking: a stack given with `--stack` starts empty after the update. The CLI creates a new signer there, or takes an
old one with `/session --mnemonic`, and you pair again. What the stack stored before isn't read. It stays in
`<base-path>/<version>/<id>`, where `<id>` is the descriptor's `id` without `-fork`, mnemonics included. A built-in
network of that name still uses that directory.
