---
"@parity/truapi-host": major
---

Native hosts consume public profiles through a cancellable finite stream with explicit resolution options. Android avatar views collect cached and refreshed metadata independently of Coil image loading and preserve the displayed image during replacement.

Breaking for Rust and Kotlin consumers: one-shot public-profile resolution is unavailable. Rust callers use `observe_public_profile(username, options)` and Kotlin callers collect `observePublicProfile(username, options)`. Swift callers use `resolvePublicProfile(username:options:)`, which returns an `AsyncThrowingStream`.
