---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

`@parity/truapi-host/markers` exports `CORE_MARKERS`, the core's marker table keyed on the dotted target, so a web host reads what may leave the device from the same list the bindings carry to iOS and Android rather than keeping a copy of its own. It sits on its own subpath so reading the table does not pull in the host runtime.
