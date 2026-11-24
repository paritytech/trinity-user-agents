---
"@parity/truapi": minor
---

`truapi-host` keeps a stack's state in a directory named after the stack's own id: its key, its URL when it has no key,
or its canonical path when it is a local file. It used the `id` in the descriptor with `-fork` cut off, so a fork
started signed in as its source's user, with the source's pairings and product data. Built-in networks keep the
directory they have. Any other stack gets `stack-<tag>`, and `network-check` prints it as `state_directory`. A local
file that is moved or renamed, or the same descriptor in another checkout or a temporary file, is another stack: it
starts with new state, as a stack the CLI has not run before, and no message says why.

A stack passed with `--stack` starts empty after the update, unless it is a built-in network given by its link or URL.
Unless a mnemonic is given, the CLI creates a new signer for it, or takes the old one with `/session --mnemonic`, and
it has to be paired again. The old data stays where it was, under the namespace the CLI printed for the stack: the
descriptor's `id` without a trailing `-fork`, or the whole `id` for `polkadot-fork`. Sessions, pairings and product
data are in `<base-path>/v2/<namespace>`, and the default session's accounts, mnemonics included, are in
`<base-path>/v2/accounts.json` under that namespace.

When that namespace is a built-in network's name, as for a fork of a built-in network or a local copy of its
descriptor with other `rpc` URLs, the built-in network keeps using what the stack stored there: it can sign with an
account attested on the stack's chains, and restore the stack's signing-host sessions and pairing-host pairings. On
the built-in network, `/session --clear <name>` removes one signing-host session, and `/session --clear-all` removes
every signing-host session, the signing-host bootstrap state and the network's accounts in `accounts.json`, its own
included. Neither removes the pairing-host state, `<base-path>/v2/<namespace>/pairing-host` and the `*_pairing_host`
directories beside it, or the `materials` cache; delete those directories to remove them.
