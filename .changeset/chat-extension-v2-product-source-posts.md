---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A `postMessage` to a room that keeps its messages in the product fails with `Denied`: the product sends those messages
with `updateFeed`. The core reads the configuration of the room the product names before each posted message, so a
host answers the default configuration for a room that it does not know, not an error.
