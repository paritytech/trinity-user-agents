---
"@parity/truapi": patch
---

A chain-head call on a follow that has ended now says what became of it, instead of reporting the id as unknown. A follow ends in two ways and the caller keeps the id through both: its connection closes under it, or the node sends a `stop` with the connection still up. Either way the id stops resolving and every call bound to it fails until a new follow is opened, and both used to read `unknown follow subscription id`, which is also what a genuinely unrecognised id returns. A scope now remembers the follows it lost, bounded, and answers `follow subscription "<id>" ended when its connection closed` or `follow subscription "<id>" was stopped by the node` as `Unavailable` rather than a `HostFailure` blaming the caller. The two are worth separating in a crash report because only one of them involves a transport at all.

A spent id is also no longer answered by an unrelated follow. Resolving an id a connection has not seen against its sole unaliased follow is how a product-minted alias first binds, so that path stays, but an id already known to have ended is excluded from it. Without that a call on a dead follow could be served from whichever follow happened to be unaliased at the time, which both hid the loss and read a block that follow had never pinned.
