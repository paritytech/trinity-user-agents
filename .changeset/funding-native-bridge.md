---
"@parity/truapi": patch
---

Native hosts drive funding through `NativeTrUApiHostRuntime`: `set_funding_callbacks` installs the overlay, and `open_funding` and `funding_session` open and read sessions. Amounts cross the FFI as decimal strings.
