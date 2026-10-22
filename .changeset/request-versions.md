---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A host refuses a request or subscription start in a version newer than it knows with an `UnsupportedVersion` protocol error, before handling it, and names the newest version it accepts. The client settles that call as `Unsupported`. A host built before this answers such a request with `MalformedFrame`.
