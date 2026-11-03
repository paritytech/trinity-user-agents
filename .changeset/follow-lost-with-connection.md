---
"@parity/truapi": patch
---

A chain-head call on a follow that has ended now says what became of it, instead of reporting the id as unknown. A follow can end while the caller still holds its id: its connection closes under it, the node sends a `stop` with the connection still up, or the subscription fails on an open connection. In each case the id stops resolving and every call bound to it fails until a new follow is opened. All of these used to read `unknown follow subscription id`, which is also what a genuinely unrecognised id returns. The failure reason now reads `follow subscription "<id>" ended when its connection closed`, `was stopped by the node`, or `ended on an error with its connection still open`. The error kind products receive is unchanged; only the reason text is. A scope remembers the follows it lost, bounded and released with the scope, under both the follow's id and any alias the product bound to it.

A spent id is also no longer answered by an unrelated follow. Resolving an id a connection has not seen against its sole unaliased follow is how a product-minted alias first binds, so that path stays, but an id already known to have ended is excluded from it. Without that a call on a dead follow could be served from whichever follow happened to be unaliased at the time, which both hid the loss and read a block that follow had never pinned.
