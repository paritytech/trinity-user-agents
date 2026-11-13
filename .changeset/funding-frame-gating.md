---
"@parity/truapi": major
---

A funding provider's screen opens only when the host opens it, never on the worker's own call.

Breaking for native hosts:

- `NativeFundingCallbacks` gains `provider_frame_requested(provider_id, intent)`. The core holds the provider's
  `presentFrame` until the host calls `open_provider_frame(intent)`: at once while the user looks at the session,
  otherwise once the user taps a notice. `present_provider_frame` is called only after that.
- `funding_frame_waiting(intent)` tells whether a screen still waits, so a host can show the notice again after
  redrawing.
