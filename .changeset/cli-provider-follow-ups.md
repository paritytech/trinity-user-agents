---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A statement allowance or PGAS transaction that the pool drops or finds invalid fails as soon as the node reports it, rather than after the two-minute submit timeout.

In the `truapi-host` CLI, a light-client chain keeps running after its last connection closes, so it syncs once per process. A `HOST_CLI_*_WS` override is checked against the preset's URL for the same chain, so pointing one chain at another chain's preset node is refused when the node reports the wrong genesis.
