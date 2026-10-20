---
"@parity/truapi": patch
---

The core can watch storage entries on a chain. `watch_entry` on a `StorageSnapshots` (implemented by `ChainRuntime`) takes any subxt storage address and its keys, reads them once per best or finalized head, and reports the decoded values: every key first, then only the keys whose value changed. Each head is one `state_queryStorageAt` call on the legacy connection, so absent entries come back as an explicit null and a key the node leaves out is an error rather than an absent value. Heads that arrive during a read collapse into the newest, metadata is fetched again only when `System.LastRuntimeUpgrade` changes, and the head subscriptions are opened again when the node ends them.
