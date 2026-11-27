---
"@parity/truapi-host": major
---

The core names the privileged products, Humanity (`peopl`), Jollity (`dim2`) and Collectibles (`stash`), on every
network. Native hosts get them from `PrivilegedProduct`, `privilegedProductId` and `privilegedProductLabel` rather than
writing the labels themselves.

The native runtime pins the Humanity card in the Pocket on its stack's network by itself, so `HostRuntimeConfig` has no
`pinnedPocketCards` field. A host that draws a pinned card before the runtime lists it reads
`pinnedPocketCards(networkSuffix)`, and `hostPlacedChats(networkSuffix)` lists the chats every host places. Web hosts
are unchanged.
