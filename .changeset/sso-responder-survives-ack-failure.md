---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A signing host keeps serving a paired host when the People chain drops a connection. It sends its acknowledgement or response again for about 15 seconds, and only then ends the session. A request whose acknowledgement never went out is answered when the restarted responder receives it again.

`truapi-host signing-host exec '/pair <link>'` keeps running through connection failures. It restarts the responder, waiting up to 30 seconds between attempts, and exits when the paired host disconnects. After a session that ran normally, the wait starts again at one second.
