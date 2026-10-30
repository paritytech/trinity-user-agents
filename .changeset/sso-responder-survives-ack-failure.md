---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A paired signing host keeps serving when its connection to the People chain drops. It retries an acknowledgement or response for about 15 seconds before it ends the session, and still answers a request it could not acknowledge once it reconnects.

`truapi-host signing-host exec '/pair <link>'` restarts the responder after a failure and exits only when the paired host disconnects. It waits 1 second before the first restart, doubling up to 30 seconds.
