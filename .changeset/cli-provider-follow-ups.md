---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A statement allowance or PGAS transaction that the pool drops or finds invalid fails as soon as the node reports it, rather than after the two-minute submit timeout.

In the `truapi-host` CLI, a light-client chain keeps running after its last connection closes, so it syncs once per process. Over RPC, a chain connection counts as dead after 50 seconds in which no data arrives at all, and a request waiting on it fails after 30 seconds in which no message arrives or makes progress, so a large message that is still downloading, such as a busy Bulletin block on a slow link, keeps both alive. The interactive status bar names the chain backend next to the network, as in `paseo-next-v2 (light-client)`.
