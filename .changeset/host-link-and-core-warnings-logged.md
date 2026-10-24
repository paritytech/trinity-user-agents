---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

The SDK warns `[truapi] host link lost: <reason>` with the close reason each time its host connection drops, including each refused redial, and the host container warns `[truapi] host link back after N ms` when the link returns, timed from the first loss. The core warns when a json-rpc reply takes more than 5 s, when a json-rpc connection closes while requests wait, naming each with its age, and when `createTransaction` waits more than 2 s for the current block or cannot get it.
