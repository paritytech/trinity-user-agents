---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A `postMessage` to a room that keeps its messages in the product fails with `Denied`: the product sends those messages
with `updateFeed`. The core reads the room the product names before each posted message (`chatRoom` on the web, `room`
in the native hosts), so a host answers no room for a room that it does not know, not an error, and the message then
reaches the host as before.
