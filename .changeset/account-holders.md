---
"@parity/truapi-host": patch
---

Product account operations on both host roles go through `HostAccounts` over an `AccountHolder`: `WalletAccountHolder` on a signing host, or `RemoteAccountHolder` on a paired host, which sends wallet operations to the phone. `ProductAuthority` is removed. `HostAccounts` binds each call to the calling product and the current wallet activation, confirms signing, transaction, VRF and allocation requests with the user before the account holder acts on them, and keeps the grants it was issued.

- A paired host answers `NotAvailable` for an allocated resource whose grant it cannot keep, instead of failing the whole allocation.
- A paired host keeps its Bulletin allowance key when a refresh fails, and replaces it once the phone issues a new one.
- The VRF signing prompt opens on the requesting product's connection, like the other signing prompts.
