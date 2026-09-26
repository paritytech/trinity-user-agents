---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `PeerTransport` host service (trait 21): host-terminated JAMNP-S QUIC or WebTransport streams to JAM peers
with `dial`, `open`, `send`, `recv`, `reset`, `close` and `events`. A host may grant it only to an execution whose
App manifest (`$v` 2) declares `capabilities.network.jam = { genesis }`, and only for that genesis; the default
implementation, including the Rust product runtime, returns `NotGranted`. Ships the browser WebTransport adapter and
the deterministic PolkaJAM certificate-hash derivation under `@parity/truapi/peer-transport`.
