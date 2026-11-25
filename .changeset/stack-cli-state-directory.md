---
"@parity/truapi": minor
---

`truapi-host` keeps a stack's state in a directory named after the stack's own id: its key, its URL when it has no key,
or its canonical path when it is a local file. It used the `id` in the descriptor with `-fork` cut off, so a fork
started signed in as its source's user, with the source's pairings and product data. Built-in networks keep the
directory they have. Any other stack gets `stack-<tag>`, and `network-check` prints it as `state_directory`. A local
file that is moved or renamed, or the same descriptor in another checkout or a temporary file, is another stack: it
starts with new state, as a stack the CLI has not run before, and no message says why.

The default session's accounts move from `<base-path>/v2/accounts.json`, which every stack shared, to
`<base-path>/v2/<state_directory>/signing-host/accounts.json`, so each stack has its own store and lock. The old file
is left in place: nothing reads it, and no clear changes it any more, so the mnemonics it holds, each under its
network's name, stay there until the file is deleted by hand, including those of sessions cleared after the update.
To take one back into a session, give its phrase to `/session --mnemonic`.

Sessions promoted to a username keep their own `accounts.json` and are not affected. The released CLI promoted every
provisioned default session into such a session and remembered it, so a host that still starts on the default session
is mostly one whose default session never finished provisioning. It creates a new auto account, with a new mnemonic
and username registration, built-in networks included. `--account <name>` selects an account in the default session's
new store, which starts empty, so a name used before the update fails with "account not found" and creates nothing;
the remembered session stays as it was. The account it named lives on, with its pairings, in the session it was
promoted into: start with `--session <username>` to use it without a new account.

A stack passed with `--stack` starts empty after the update, unless it is a built-in network given by its link or URL.
Devices paired with it have to be paired again. The CLI creates a new signer for it, unless `--mnemonic` or
`HOST_CLI_SIGNER_MNEMONIC` gives one.

The old data stays where it was, under the name the released `network-check` printed as `network`: the descriptor's
`id` without a trailing `-fork`, or the whole `id` for `polkadot-fork`. Sessions, pairings and product data are in
`<base-path>/v2/<network>`. The old signers' mnemonics are in `<base-path>/v2/accounts.json` under that name for the
default session, and in `<base-path>/v2/<network>/<name>_signing_host/accounts.json` for a named session. To keep an
old signer, give its phrase to `/session --mnemonic`.

When that name is a built-in network's, as for a fork of a built-in network or a local copy of its descriptor with
other `rpc` URLs, the built-in network keeps using what the stack stored there: it can restore the stack's named
signing-host sessions, which sign with accounts attested on the stack's chains, and its pairing-host pairings. On the
built-in network, `/session --clear <name>` removes one signing-host session, and `/session --clear-all` removes every
signing-host session and the signing-host bootstrap state. Neither removes the pairing-host state,
`<base-path>/v2/<network>/pairing-host` and the `*_pairing_host` directories beside it, or the `materials` cache;
delete those directories to remove them.
