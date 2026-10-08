---
"@parity/truapi": major
"@parity/truapi-host": major
---

`chat.createRoom` takes an optional `hideTextInput`, and a host shows the room without its text field when it is `true`. The request is now v0.2, so a host that has not updated cannot decode it and every `createRoom` from an updated product fails there. Native hosts receive the flag as a new `hideTextInput` argument of `ChatHostBridge.createRoom`, applied to an existing room as well as a new one.
