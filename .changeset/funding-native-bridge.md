---
"@parity/truapi": patch
---

Native hosts drive funding through `NativeTrUApiHostRuntime`: `set_funding_callbacks` installs the overlay, `set_top_up_callbacks` and `notify_top_up_status` the top-up engine, and `open_funding` and `funding_session` open and read sessions. Amounts cross the FFI as decimal strings.
