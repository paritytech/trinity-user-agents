---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A paired signing host keeps serving when its connection to the People chain drops. It retries an acknowledgement, a pairing answer or a disconnect notice for about 15 seconds. If an acknowledgement still fails, it ends the session and answers the request once it reconnects. It retries a response until the People chain takes it, because the request has already run.

`truapi-host signing-host exec '/pair <link>'` restarts the responder after a failure and exits only when the paired host disconnects. It waits 1 second before the first restart, doubling up to 30 seconds.
