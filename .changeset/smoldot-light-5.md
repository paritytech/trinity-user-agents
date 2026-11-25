---
"@parity/truapi-provider": patch
---

The embedded light client builds on smoldot 7 and smoldot-light 5, so legacy runtime calls such as `state_call` reuse a
runtime the client already holds instead of downloading the runtime code again at every new block.
