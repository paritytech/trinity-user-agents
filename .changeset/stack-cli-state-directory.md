---
"@parity/truapi": minor
---

A fork passed to `truapi-host --stack` started signed in as its source's user, with the source's pairings and product
data. The CLI named a stack's state directory after the descriptor's `id` with `-fork` cut off, so the fork and its
source shared one directory. Now the name comes from the stack's own id: its key, its URL if it has no key, or its
canonical path if it's a local file. Built-in networks keep the directory they have. Any other stack gets `stack-<tag>`,
and `network-check` prints it as `state_directory`.

So after the update a `--stack` starts empty, unless it's a built-in network given by its link or URL. Pair its devices
again. The CLI creates a new signer for it unless `--mnemonic` or `HOST_CLI_SIGNER_MNEMONIC` gives one. A local file is
identified by its path, so the same descriptor at another path starts empty too, and the CLI doesn't say why.

The default session's accounts move from `<base-path>/v2/accounts.json`, which every stack shared, to
`<base-path>/v2/<state_directory>/signing-host/accounts.json`. Sessions promoted to a username keep their own
`accounts.json`. That's most of them, because the released CLI promoted every default session that finished provisioning
and starts in it by default. A host that still starts on the default session gets a new auto account, with a new
mnemonic and username registration, on built-in networks too. `--account <name>` looks in the new store, so a name from
before the update fails with "account not found". That account is in the session it was promoted into, with its
pairings. Start with `--session <username>` to use it.

The old data stays in `<base-path>/v2/<network>`, where `<network>` is what the released `network-check` printed as
`network`: the descriptor's `id` without a trailing `-fork`, or the whole `id` for `polkadot-fork`. The old signers'
mnemonics are in `<base-path>/v2/accounts.json` under that name for the default session, and in
`<base-path>/v2/<network>/<name>_signing_host/accounts.json` for a named session. To keep a signer, give its phrase to
`/session --mnemonic`. Nothing reads or clears `<base-path>/v2/accounts.json` any more, so delete it once you don't need
the phrases in it.

If `<network>` is a built-in network's name, that network still uses the directory. This is the case for a fork of a
built-in network, and for a local copy of its descriptor with other `rpc` URLs. The built-in network can restore the
stack's named signing-host sessions, which sign with accounts attested on the stack's chains, and its pairing-host
pairings. `/session --clear <name>` and `/session --clear-all` remove the sessions there. No command removes
`<base-path>/v2/<network>/pairing-host`, the `*_pairing_host` directories beside it or the `materials` cache, so delete
those by hand.
