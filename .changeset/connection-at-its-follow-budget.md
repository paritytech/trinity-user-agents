---
"@parity/truapi": patch
---

A chain connection that reaches the node's follow budget says so, naming its chain, the count and every follow holding a slot by its local id. A node allows four `chainHead_v1_follow` per connection and refuses the next one whoever it belongs to, and that refusal surfaces far from the cause: the call that happens to open the next follow fails, often as a metadata error that mentions no follows at all. Naming the occupants while they are still known puts the cause in the record rather than only the consequence.

Only follows the node has acknowledged are counted. A follow is recorded on intent and gains a remote id once the node answers, so counting intents would report a connection as full while the node still had room. The count is the core's own bookkeeping rather than the node's.

These reach a host through its log sink rather than the console. The level filter defaults to `ERROR` so routine core traffic stays off a host's log, which also means an ordinary `warn!` never reaches a device. An event whose target starts with `truapi.` is addressed to the host instead, and a layer forwards those to the sink under its own target filter rather than the reloadable level, so they arrive at the default floor without turning on the rest of the crate's warn traffic. Native hosts install their existing `on_core_log` callback as that sink.

A host runtime raises a start marker as it comes up, so a host log opens with evidence that the channel carrying the rest is alive.

A product connection reports how many connections its execution holds as it opens and as it closes, so a host log tells a socket that was released from one that is still held.

A dotNS lookup's follow id names what it reads rather than whose account it reads, so the ids these lines carry hold no account.
