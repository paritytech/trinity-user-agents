---
"@parity/truapi": patch
---

The core can watch storage entries on a chain. `watch_entry` on a `WatchedStorage` (implemented by `ChainRuntime`) takes any subxt storage address and its keys and reports the decoded values: every key first, then only the keys whose value changed. A finalized watch reads every key at each finalized head with one `state_queryStorageAt` call and compares it with the previous read; heads that arrive during a read collapse into the newest. A best watch holds one `state_subscribeStorage` and reports its notifications, dropping keys a block wrote with the value they already had. Absent entries come back as an explicit null, and a key the node leaves out of a full answer is an error rather than an absent value. Metadata is fetched again only when `System.LastRuntimeUpgrade` changes, and subscriptions the node ends are opened again.
