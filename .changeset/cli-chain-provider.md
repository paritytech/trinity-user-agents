---
"@parity/truapi": minor
---

Every chain connection the `truapi-host` CLI makes goes through `truapi-provider`. `--chain-backend light-client` (or `TRUAPI_CHAIN_BACKEND=light-client`) syncs the chains with the embedded smoldot light client instead of dialing the preset's RPC nodes, so a node that refuses a shared CI address no longer stops the host. RPC stays the default. The light client supports `paseo-next-v2` and `previewnet`. A request from a CLI command fails after 60 seconds without an answer.

Over RPC, a connection sends a ping after 20 seconds without an outgoing request, and counts as dead after 50 seconds without any incoming frame, so the core opens a new connection instead of waiting on a dead one. Messages of up to 64 MiB pass in either direction, so the full body of a busy Bulletin block can be fetched. Chain connections verify `wss://` certificates with the root certificates built into the binary, not with certificate authorities added to the operating system.
