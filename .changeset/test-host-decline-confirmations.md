---
"@parity/truapi-host": minor
---

`setConfirmUserActions(confirm)` on the mock host and the Playwright `testHost`, and `set_confirm_user_actions` on the Rust `MockPlatform`, change how reviews are answered while a test runs, so one test can decline a signature without a fixture booted with `confirmUserActions: false`. A permission the core has cached keeps its answer, and `reset` restores the configured value.
