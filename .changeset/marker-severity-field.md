---
"@parity/truapi": patch
---

`MarkerClass::Export` names its severity field, so a host reading the marker table over the bindings gets `markerClass.severity` in Kotlin and `case .export(severity:)` in Swift rather than a positional payload.
