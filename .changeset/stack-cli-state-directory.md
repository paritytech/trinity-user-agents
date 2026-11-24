---
"@parity/truapi": minor
---

`truapi-host` keeps a stack's state in a directory named after the stack's own id: its key, its URL when it has no key,
or its path when it is a local file. It used the `id` in the descriptor with `-fork` cut off, so a fork started signed
in as its source's user, with the source's pairings and product data. Built-in networks keep the directory they have.
Any other stack gets `stack-<tag>`, and `network-check` prints it as `state_directory`.

A stack passed with `--stack` starts empty after the update. The CLI creates a new signer for it, or takes the old one
with `/session --mnemonic`, and it has to be paired again. The old data stays in `<base-path>/<version>/<id>`, which for
a fork is its source's directory, and the accounts saved for it stay in `accounts.json` under that old name.
