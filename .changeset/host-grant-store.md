---
"@parity/truapi-host": patch
---

Keep the pairing host's and the signing host's grants in one `HostGrantStore`. Resetting a product no longer ends other products' calls on a signing host or a session activation in progress on a paired host; it still discards grants that a request began issuing before the reset, including an allocation confirmed while the reset happened. A paired host now retries a failed deletion of a disconnected session's stored grants before the next session is installed.
