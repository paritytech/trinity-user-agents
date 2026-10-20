---
"@parity/truapi-host": patch
---

Statement submissions and the Statement Store allowance paths run over the People-chain connection the chain runtime keeps open, so publishing a statement costs no WebSocket handshake. Statement subscriptions open their own connection.
