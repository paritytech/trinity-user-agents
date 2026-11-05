---
"@parity/truapi-host": patch
"@parity/truapi-provider": patch
---

Refactor the Rust host runtime and browser provider to the workspace Rust guidance: unit tests live in their own files, lint suppressions are removed by fixing their causes, unused functions and derives are dropped, and truncating casts are checked. The JavaScript API and wire protocol are unchanged.
