---
"@parity/truapi": patch
---

Native hosts drive funding through `NativeTrUApiHostRuntime`: `set_funding_callbacks` installs the overlay, `set_top_up_callbacks` and `notify_top_up_status` the top-up engine, and `enable_funding_conversion`, `open_funding`, `quote_funding_deposit`, `assign_funding_deposit` and `funding_session` run the on-ramp. Amounts cross the FFI as decimal strings.
