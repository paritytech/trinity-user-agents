---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `PeerTransport` host service (trait 23): host-terminated JAMNP-S QUIC or WebTransport streams to JAM peers
with `dial`, `open`, `send`, `recv`, `reset`, `close` and `events`. Access is the runtime permission
`RemotePermission::JamPeers { genesis }`, appended as variant 5: before a `dial` connects, the host checks the
product's stored decision, prompts when it is undetermined and persists the answer per product and genesis. App
manifests declare nothing. The default implementation, including the Rust product runtime, returns `NotGranted`.
Ships the browser WebTransport adapter, whose `createPeerTransportSession({ authorize })` asks once per genesis per
session, and the deterministic PolkaJAM certificate-hash derivation under `@parity/truapi/peer-transport`.
