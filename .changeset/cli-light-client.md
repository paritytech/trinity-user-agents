---
"@parity/truapi-host": patch
---

The `truapi-host` CLI can reach the chains through the embedded smoldot light client instead of the public RPC nodes: set `TRUAPI_LIGHT_CLIENT=1`. The host's own chain traffic (statement store, allowance renewal, preimages, PGAS claims) then needs no RPC node, so a node that rate-limits a shared CI address no longer stops the host at startup. RPC stays the default.
