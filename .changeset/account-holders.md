---
"@parity/truapi-host": major
---

Product account operations on both host roles go through `HostAccounts` over an `AccountHolder`: `WalletAccountHolder` on a signing host, or `RemoteAccountHolder` on a paired host, which forwards each request to the phone. `ProductAuthority` is removed. `HostAccounts` binds each call to the calling product and the current wallet activation, confirms signing, transaction, VRF and allocation requests with the user before the account holder acts on them, and keeps the grants it was issued. Bulletin submissions check the authorization and ask for an increase before submitting instead of retrying after a rejection. Raw signing through the legacy-account API tells the product's own slot-zero account apart from the active identity.
