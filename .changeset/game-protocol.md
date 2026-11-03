---
"@parity/truapi": minor
---

Add the `game` service: `remindNextGame` holds one reminder for the calling product's next game, and `cancelNextGame`
drops it. Only the game product, `dim2`, is served; any other product gets `Unsupported`. A host that fails to hold
the reminder answers with a host failure carrying its reason. The game product needs no per-product consent: the host
asks the OS for what the reminder needs, ringing an alarm where it can and delivering a notification otherwise.
