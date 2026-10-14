---
"@parity/truapi-host": major
---

Product account operations on both host roles go through `HostAccounts` over an `AccountHolder`: the wallet on a signing host, or an SSO client on a paired host that forwards each request to the phone. The host running the product confirms signing, transaction, VRF and allocation requests with the user, and the phone confirms a forwarded one again. Approval, signing and allowance issuance are bound to the wallet activation that started them. One `ProductConsent` per runtime decides account access: Allow once answers the same request until the app restarts, the wallet locks or changes, or the product is reset, and one account-access prompt runs at a time. A native wallet with its own SSO transport opens one service per peer with `openSsoService`, which replaces `handleSsoRequest` on the host runtime.
