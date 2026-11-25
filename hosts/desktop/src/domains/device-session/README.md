# device-session

The `device-session` domain owns the **encrypted device-to-device signaling channel**: a directional, ACK'd session over the
statement-store through which two of a user's devices (or a device and PApp) exchange WebRTC signaling. It owns the payload
crypto, the pairwise topic derivation, the wire envelope, and the batching/retry semantics that make delivery reliable over a
replace-only statement store.

The unit of consumption is a `DeviceSessionChannel` — `send(envelope)` / `messages$` / `close()`. It is deliberately a
**transport** layer: it moves opaque signaling envelopes between two devices reliably and confidentially, and knows nothing about
_why_ they sync. The actual sync orchestration (who to connect to, when) is `device-sync`'s job; this domain just gives it a pipe.

## Vocabulary

- **Device session** — a confidential channel between exactly two devices, keyed by X25519 key agreement over their encryption keys. Not to
  be confused with a chat session (`chat`) or an SDK `UserSession` (`application`).
- **`DeviceSessionChannel`** — the consumer-facing handle: `send`, `messages$` (inbound `SyncSignalingEnvelope` stream), `close`.
  Type in `types.ts`; built by `deviceSessionUseCase.createChannel(deps)`.
- **Directional topic** — each direction has its own statement-store topic: the sender posts to one, the receiver subscribes to
  the mirror. Derived by `deviceSessionService.deriveDeviceSessionTopic` (byte-equivalent to Android's
  `deriveCommunicationTopic`).
- **`SignalingStatementData`** — the domain's wire boundary codec (`schemas.ts`): a SCALE `Enum` of `Request` (a `requestId` +
  the full unacked batch of opaque messages) and `Response` (an ACK: `requestId` + `responseCode`). Its discriminants (0/1) match
  Android's `StructuredStatementData` and chat's codec so the wire is interoperable — see Boundaries.
- **Unacked batch** — because a new `Request` statement _replaces_ the previous one on the topic, each `Request` carries every
  signaling message the peer has not yet ACK'd; a covering `Response` drops exactly those. This is how a late-polling peer still
  sees the Offer and not just the trailing ICE candidates.

## Scope

This domain owns:

- **Payload encryption** — ChaCha20-Poly1305 keyed via HKDF-SHA256 over X25519(devEncPriv, peerDevEncPub), byte-compatible with
  Android (`deviceSessionService.deriveSharedSecret` + `createPayloadEncryption`, built once per channel). See CHAT-RFC-0004.
- **Topic derivation** — the directional `khash("session" || sender || receiver || "//")` topics.
- **The signaling wire envelope** — `SignalingStatementData` (`Request`/`Response`), validated at the decode boundary.
- **Reliable delivery semantics** — the unacked-batch, ACK-every-Request, dedupe, and last-Offer-wins rules in
  `deviceSessionUseCase.createChannel`.

## Opening and using a channel

`deviceSessionUseCase.createChannel(deps)` takes the two devices' keys/account-ids plus injected `post` / `subscribe` transport
functions (the caller supplies the statement-store binding — this domain does no I/O itself), and returns a `DeviceSessionChannel`.
Send with `channel.send(envelope)`; read inbound envelopes from `channel.messages$`; always `channel.close()` to unsubscribe.

Rule of thumb: this domain gives you a confidential, reliable pipe between two devices; deciding _when_ to open it and _what_ to
sync is the caller's (`device-sync`'s) concern.

## Boundaries

This domain does **not** own:

- **Sync orchestration** — which peers to connect to, the WebRTC state machine, and the sync payloads live in `device-sync`, which
  _uses_ this channel.
- **The chat envelope** — chat's full `StructuredStatementData` (with its multi-device `MultiRequest`/`MultiResponse` variants)
  stays in `chat/p2p/requests/schemas.ts`. This domain deliberately defines its **own** minimal `Request`/`Response` codec rather
  than importing chat's, keeping the wire discriminants (0/1) aligned by contract — never by a cross-domain import.
- **The statement-store transport** — `post` / `subscribe` are injected by the caller; the store binding is owned upstream.
- **Device / user keys** — the X25519 and sr25519 key material comes from the `device` domain; this domain only consumes it.

## References

- [`@noble/ciphers`](https://github.com/paulmillr/noble-ciphers) / [`@noble/curves`](https://github.com/paulmillr/noble-curves) /
  [`@noble/hashes`](https://github.com/paulmillr/noble-hashes) — X25519 key agreement and HKDF/blake2; the AEAD itself comes from the SDK's `createEncryption`.
- [`@novasamatech/statement-store`](https://www.npmjs.com/package/@novasamatech/statement-store) — `createSessionId` / `khash`
  backing the topic derivation.
- [`@/shared/peer-channel`](../../shared/peer-channel) — `SyncSignalingEnvelopeCodec`, the inner signaling envelope carried in the
  batch.
- [`chat/p2p/requests/schemas.ts`](../chat/p2p/requests/schemas.ts) — the sibling `StructuredStatementData` this domain's wire
  codec must stay byte-compatible with.
