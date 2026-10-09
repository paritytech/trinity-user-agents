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

The browser adapter pins two certificates per validity period: PolkaJAM's stock serial-0 certificate and one whose
serial is derived from the peer's P-256 key and period (first 8 bytes of SHA-256(compressed key ‖ period as big-endian
u64), top bit cleared, 1 if zero). Firefox's NSS rejects a second certificate with the same issuer and serial, so with
stock nodes it reaches one validator; nodes that use the derived serial are all reachable, and stock nodes keep working.

Receive queues reserve length-prefixed message bytes against the per-connection budget before allocating payloads,
including empty messages, and apply backpressure until the guest drains or resets a stream. Pending opens reserve stream
slots before transport setup; cancelled or closed operations cannot publish late streams. Browser cancellation also
settles blocked stream opens and writes, not only dials. Cancelled browser writes retain their connection quota until
the underlying sink releases the buffered frame.
Native stream senders carry reset-on-drop guards from asynchronous opening onward, including results abandoned before
the caller observes them.
Native and browser dials reserve from the eight-connection budget before awaiting permission, and retain at most eight distinct
genesis decisions per execution, including pending and refused decisions. A new ninth network returns `Limit`, without
evicting or re-prompting old decisions. Cancellation frees its operation slot and removes its decision subscriber.

The full genesis scopes permission decisions, not cryptographic validator membership. Native QUIC negotiates the
JAMNP-S ALPN containing a genesis prefix; WebTransport uses HTTP/3, and the current PolkaJAM CONNECT endpoint does not
negotiate a genesis. Both transports pin caller-supplied peer keys. Guests must verify chain data themselves; access
permits sending as well as receiving peer messages and is not read-only.

Android and iOS consent now describe bidirectional messaging and display the complete genesis in the permission
prompt, including batched requests, and in permission details. Short summary titles do not replace the full identity.

JAM consent uses the shared canonical product permission authority, including revision fences and revocation of
execution-local grants, while preserving the account-neutral product/genesis scope.
