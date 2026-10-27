---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A call made while the SDK has lost its host connection waits for the connection it redials and is sent once that connection completes its handshake. A call or subscription whose frame already reached the host fails with `ConnectionResetError`, since the host may have started it. The waiting call fails with the same error when the SDK stops redialing: the page is hidden, the retries are spent, or the connection was disposed. The 120 s request deadline still bounds the wait.
