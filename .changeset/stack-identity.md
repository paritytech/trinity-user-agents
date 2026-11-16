---
"@parity/truapi-host": major
---

A `TestStack` needs an `id` and a `generation`: the stack's identity and the run of its chains, which a host with a
bundled descriptor takes from the stack's pointer and descriptor. `mockTestStack()` uses `mock` for both. A stack the
fixture builds from `networks`, or `liveChain` returns, names its `generation` after its genesis hashes in lowercase,
and keeps `mock` while those are the mock chains.
