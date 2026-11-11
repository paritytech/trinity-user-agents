---
"@parity/truapi-host": major
---

A native wallet with its own SSO transport opens one service per peer with `openSsoService`, which replaces `handleSsoRequest` on the host runtime. Each service is bound to the wallet activation that authenticated the peer, so after a lock or wallet switch it answers not-connected instead of using the new wallet. `handleSsoControl` applies a `Cancel` at once, so it reaches the request it withdraws instead of waiting behind it.
