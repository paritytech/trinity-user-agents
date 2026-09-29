---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `JamPeerTransport` host service (trait 111): host-terminated JAMNP-S QUIC or WebTransport streams to JAM peers
with `dial`, `open`, `send`, `recv`, `reset`, `close` and `events`. Access is the runtime permission
`RemotePermission::JamPeers { genesis }`, appended as variant 5: before a `dial` connects, the host checks the
product's stored decision, prompts when it is undetermined and persists the answer per product and genesis. App
manifests declare nothing. The trait's default implementation returns `NotGranted`, and the browser core keeps it.
Ships the browser WebTransport adapter, whose `createJamPeerTransportSession({ authorize })` asks once per genesis per
session, and the deterministic PolkaJAM certificate-hash derivation under `@parity/truapi/jam-peer-transport`. Native
Rust product runtimes (iOS, Android, CLI) serve the service over JAMNP-S QUIC with the same session rules, and the iOS
and Android hosts show their remote-permission prompt for `JamPeers`.
