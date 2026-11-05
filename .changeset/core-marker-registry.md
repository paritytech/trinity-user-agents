---
"@parity/truapi": patch
---

Every marker the core addresses to a host is listed in one table, `truapi::markers`, which says whether it may leave the device and at what severity. Call sites name a marker from that table rather than writing its target out, so a marker the table does not list does not build, and the layer refuses one that reaches it anyway. `coreMarkers()` hands the table to a host, which reads it once and looks up what it is handed instead of matching prefixes of its own.
