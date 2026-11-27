---
"@parity/truapi": minor
---

Each stack now has its own state directory under `<base-path>/v2`, so a fork no longer starts with its source's user,
pairings and product data. The built-in networks keep their directories. `network-check` prints the name as
`state_directory`.

The CLI runs its stack through the core, as the apps do. A built-in network starts from the copy this release ships, or
from a newer one kept in its state directory, instead of waiting on the published descriptor every time, and a signing
or pairing host refreshes it while it runs. A `--stack` link or URL is fetched by the first command and kept there too.
When a chain gives a genesis hash the descriptor does not name, the CLI fetches the descriptor again before it fails.

Breaking: a `--stack` that isn't a built-in network starts empty after the update, with a new signer and no pairings.
The default session's accounts are kept per stack too, so a host still on the default session gets a new auto account,
and an `--account` name from before the update fails. Start with `--session <username>` instead. Old data isn't deleted,
and an old signer can be imported with `/session --mnemonic`.
