---
"@parity/truapi": patch
---

A chain connection that reaches the node's follow budget says so, naming the chain, the count and every follow holding a slot by its local id. A node allows four `chainHead_v1_follow` per connection and refuses the next one whoever it belongs to, and that refusal surfaces far from the cause: the call that happens to open the next follow fails, often as a metadata error that mentions no follows at all. The warning puts the occupants on the record while they are still legible, so a report carries the list rather than only the consequence.

Only follows the node has acknowledged are counted. A follow is recorded on intent and gains a remote id once the node answers, so counting intents would report a connection as full while the node still had room.
