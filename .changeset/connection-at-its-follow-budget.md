---
"@parity/truapi": patch
---

A chain connection that reaches the node's follow budget says so, naming the chain, the count and every follow holding a slot by its local id. A node allows four `chainHead_v1_follow` per connection and refuses the next one whoever it belongs to, and that refusal surfaces far from the cause: the call that happens to open the next follow fails, often as a metadata error that mentions no follows at all. Naming the occupants while they are still known puts the cause in the record rather than only the consequence.

Only follows the node has acknowledged are counted. A follow is recorded on intent and gains a remote id once the node answers, so counting intents would report a connection as full while the node still had room.

This arrives as a core notice rather than a log line. The level filter defaults to `ERROR` so routine traffic stays off a host's log, which also means a `warn!` from the core never reaches a device. `logging::notice` sends the few conditions worth a host log entry on their own to a sink the host installs, and to tracing as well for hosts that read that instead. Native hosts are wired to their existing `on_core_log` callback, so the notice reaches a device log with no change to the trace level.
