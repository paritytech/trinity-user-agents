---
"@parity/truapi": major
"@parity/truapi-host": major
---

A marker that records something going normally stays on the device. `Severity` carries `Warn` and `Error`, so a marker recording normal progress cannot be declared exportable at all, and the twenty that were are `Local`: `truapi.core.started`, five `truapi.ws_bridge.*` and fourteen `truapi.funding.*` facts. `truapi.funding.ended_unexpectedly` stays `Export(Error)` so a fault can still be alerted on. Nineteen markers are exported, seventeen at warn and two at error.

The marker table `@parity/truapi-host` publishes under `./markers` narrows with it. `MarkerSeverity` is `"warn" | "error"`, so a host matching on `"info"` drops that branch.
