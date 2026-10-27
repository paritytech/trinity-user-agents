---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

The core closes a chain connection on which a request has waited 30 s with no frame arriving. The waiting requests fail with `json-rpc connection closed: no frame for 30s while a request waited`, the host is told to close the socket, and the next call opens a new connection without waiting for the host to miss a pong. The deadline outlasts the largest frame a chain sends on a slow link, the 1.3 MB Asset Hub runtime metadata, so a slow answer is not mistaken for a dead socket.
