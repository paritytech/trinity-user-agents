---
"@parity/truapi-host": major
"@parity/truapi": major
---

Experimental host account-holder separation introduces typed secret-storage callbacks, identifier-based native wallet activation, asynchronous native lifecycle methods and Rust-owned native runtime repositories. Custom host adapters must implement the new protected storage contract. Existing product and SSO wire messages retain their encodings. Persisted data from older host storage formats is not imported.
