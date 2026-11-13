---
"@parity/truapi": major
"@parity/truapi-host": major
---

The game product, Jollity (`dim2` on every network), reads the DotNS usernames of the unblocked chat contacts of the user with `contacts.usernames({})`, which answers `{ usernames }` without a prompt. Any other product, and a host that lists no usernames, gets `Unsupported`. While nobody is signed in, Jollity gets `NotConnected`.

A JS host serving contacts passes `contactUsernames` beside `contacts` and `pickContact`. The `contacts` host callbacks now require `contactUsernames`, so a TypeScript host that serves contacts fails to compile until it adds it, and one that cannot list usernames returns `{ tag: "Unsupported" }`. Native hosts get `contactUsernames` on `ContactsHostBridge` with a default that answers `Unsupported`.
