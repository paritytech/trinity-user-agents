---
"@parity/truapi-provider": patch
---

Browser WebSocket handlers are detached before their Rust closures are released, including failed or cancelled connection attempts, preventing late events from invoking dropped callbacks. Light-client socket callbacks release their state lock before waking consumers.
