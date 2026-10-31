---
"@parity/truapi": patch
---

`coreMarkers()` hands a host every marker the core may address to it, keyed by target, with whether each may leave the device and at what severity. A host reads it once and looks up the markers it is handed, instead of keeping its own list of exported prefixes.
