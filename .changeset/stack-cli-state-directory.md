---
"@parity/truapi": minor
---

Each stack now has its own state directory under `<base-path>/v2`, so a fork no longer starts with its source's user,
pairings and product data. The built-in networks keep their directories. `network-check` prints the name as
`state_directory`.

Breaking: a `--stack` that isn't a built-in network starts empty after the update, with a new signer and no pairings.
The default session's accounts are kept per stack too, so a host still on the default session gets a new auto account,
and an `--account` name from before the update fails. Start with `--session <username>` instead. Old data isn't deleted,
and an old signer can be imported with `/session --mnemonic`.
