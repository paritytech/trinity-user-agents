---
"@parity/truapi-host": patch
---

Statement submissions and the Statement Store allowance paths run over the People-chain connection the chain runtime keeps open, so publishing a statement costs no WebSocket handshake. A submission left unanswered for 15 seconds closes that connection, and a submission whose connection closed or stalled is sent once more over a fresh one. Statement subscriptions open their own connection.
