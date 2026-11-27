---
"@parity/truapi": minor
---

Each stack now has its own state directory under `<base-path>/v2`, so a fork no longer starts with its source's user,
pairings and product data. The built-in networks keep their directories. `network-check` prints the name as
`state_directory`.

The CLI runs its stack through the core, as the apps do. A built-in network starts from the copy this release ships, or
from a newer one kept in its state directory, and a command fetches the published descriptor first, waiting up to 3
seconds, only when that copy was last fetched over an hour ago or never. A `--stack` link or URL is fetched by the first
command and kept there too. When an endpoint or light spec fails the checks, the CLI fetches the descriptor again before
it fails. The signing host also runs NFT backup recovery, as the apps do, since the stack it runs now names the
descriptor's `accountDataStore` contract.

Breaking: a `--stack` that isn't a built-in network starts empty after the update, with a new signer and no pairings.
The default session's accounts are kept per stack too, so a host still on the default session gets a new auto account,
and an `--account` name from before the update fails. Start with `--session <username>` instead. Old data isn't deleted,
and an old signer can be imported with `/session --mnemonic`.
