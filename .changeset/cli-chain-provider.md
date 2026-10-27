---
"@parity/truapi": minor
---

The `truapi-host` CLI opens its chain connections through one `truapi-provider` chain provider: the host's own chain traffic, dotNS username reads and Bulletin preimage lookups. `--chain-backend light-client`, or `TRUAPI_CHAIN_BACKEND=light-client`, makes it sync the chains with the embedded smoldot light client instead of dialing the preset's RPC nodes, so a node that refuses a shared CI address no longer stops the host. RPC stays the default. The light client supports `paseo-next-v2` and `previewnet`, cannot be combined with the `HOST_CLI_*_WS` overrides or `HOST_CLI_LOCAL_NETWORK_CONFIG`, and makes a chain's first request wait for the chain to sync. Account setup's ring-membership poll, `register-name`'s transactions, `alloc-check` and `pgas-check` still use the RPC nodes.

Over RPC, a connection sends a ping after 20 seconds without an outgoing request, and counts as dead after 50 seconds without any incoming frame, so the core opens a new connection instead of waiting on a dead one. Messages of up to 64 MiB pass in either direction, so the full body of a busy Bulletin block can be fetched. Chain connections verify `wss://` certificates with the root certificates built into the binary. Certificate authorities added to the operating system are not used.
