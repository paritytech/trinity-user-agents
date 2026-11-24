---
"@parity/truapi": minor
---

`truapi-host` keeps a stack's state in a directory named after the stack's own id: its key, its URL when it has no key,
or its canonical path when it is a local file. It used the `id` in the descriptor with `-fork` cut off, so a fork
started signed in as its source's user, with the source's pairings and product data. Built-in networks keep the
directory they have. Any other stack gets `stack-<tag>`, and `network-check` prints it as `state_directory`. A local
file that is moved or renamed, or the same descriptor in another checkout or a temporary file, is another stack: it
starts with new state and a new signer, and the CLI prints nothing about it.

A stack passed with `--stack` starts empty after the update, unless it is a built-in network given by its link or URL.
The CLI creates a new signer for it, or takes the old one with `/session --mnemonic`, and it has to be paired again.
The old data stays where it was, under the descriptor's `id` without `-fork`: sessions, pairings and product data in
`<base-path>/v2/<id>`, and the default session's accounts, mnemonics included, in `<base-path>/v2/accounts.json`.
For a fork of a built-in network, `<id>` is that network's name, so the built-in network keeps using what the fork
stored there: it can sign with an account the fork attested on the fork's chains, and restore a session the fork
created. On the built-in network, `/session --clear <name>` removes one such session, and `/session --clear-all`
removes everything stored for the network, its own sessions and accounts included.
