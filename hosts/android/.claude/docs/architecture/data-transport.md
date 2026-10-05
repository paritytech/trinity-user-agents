# Data Transport (WebRTC)

`DataTransport` is a reusable peer-to-peer transport built on a single WebRTC data channel with SCALE-multiplexed messaging. It is the right tool for **streaming or latency-sensitive P2P** between two users (calls, in-game messages). For one-shot, encrypted, chain-mediated rendezvous, use statement-store communication instead — see `architecture/statement-store-communication.md`.

## Rules at a glance

1. **`blocking`** — Reusing a reserved `UseCaseId` (`webrtc_renegotiation_internal_use_case`, `webrtc_media_state_use_case`). Pick a new feature-prefixed id.
2. **`major`** — Signaling logic inside the transport — keep it behind a `PeerChannelSignaling` impl owned by the consumer.
3. **`major`** — Consumer reaching into the underlying WebRTC `DataChannel` directly; the only entry points are `subscribeMessages` / `send`.
4. **`major`** — Calling `send(...)` before `awaitOpen()` (or without an `isOpen()` guard).
5. **`major`** — Custom reconnect logic inside the transport; the consumer chooses the policy.
6. **`major`** — Mixing media tracks and data-channel payloads through the same API — media is `MediaTrackProvider`, data is `DataTransport`.
7. **`major`** — Raw text / JSON payloads — always `@Serializable` + `BinaryScale`.
8. **`minor`** — Hardcoded use-case id literal at the call site instead of a `const val`.

---

## The interface

`tools/media-connection/api/.../domain/DataTransport.kt`

```kotlin
typealias UseCaseId = String
typealias UseCaseData = ByteArray

interface DataTransport {
    val state: StateFlow<DataTransportState>

    fun subscribeMessages(id: UseCaseId): Flow<UseCaseData>
    suspend fun send(id: UseCaseId, data: UseCaseData)
    suspend fun awaitOpen()
    fun isOpen(): Boolean
}

enum class DataTransportState { Connecting, Open, Closing, Closed }
```

Single data channel, multiple `UseCaseId` namespaces. Messages are SCALE-encoded `DataChannelMessage(id, data)` so multiple feature concerns can share one WebRTC connection without interleaving.

---

## What's generic, what isn't

The transport's job ends at "open WebRTC, multiplex by `UseCaseId`, exchange bytes". Everything specific to *how peers find each other* is the caller's job, via a `PeerChannelSignaling` implementation.

| Concern | Owned by | Notes |
|---|---|---|
| Data channel lifecycle | `DataTransport` / `DataChannelMessaging` | open / close / state |
| Multiplexing | `DataTransport` | by `UseCaseId` |
| Wire encoding | `DataTransport` | `DataChannelMessage` SCALE |
| Reliability / retransmit | WebRTC stack | built-in for ordered data channels |
| SDP / ICE exchange | **Consumer** via `PeerChannelSignaling` | chat messages (calls) or statement-store sessions (games) |
| Encryption of signaling | **Consumer** | chat encryption / `CommunicationEncryption` |
| Media tracks (audio/video) | **Consumer** | for calls; games skip |
| Reconnection policy | **Consumer** | transport does not auto-reconnect |
| Codec selection | **Consumer** | calls; games skip |

Two distinct signaling channels exist today:

- **External signaling** (calls): SDP and ICE candidates ride on **chat messages** (`ChatMessage.Content.DataChannelOffer/Answer/IceCandidate/Closed`). The "purpose" field distinguishes audio vs video calls.
- **Internal signaling** (games): SDP and ICE ride on a **statement-store `CommunicationSession`** with `SignalingEnvelope(gameIndex, offerId, message)` payloads.

Both are valid; pick based on whether the peers are already in a chat together (calls) or need a chain-mediated rendezvous (games).

---

## Implementation primitives

`tools/media-connection/impl/.../`:

- **`RealDataTransport`** — thin wrapper, delegates to `DataChannelMessaging` for state and I/O.
- **`DataChannelMessaging`** — owns the WebRTC `DataChannel`, encodes/decodes `DataChannelMessage`, exposes state and message flow.
- **`InitiatorConnection`** / **`AcceptorConnection`** — own the underlying `PeerConnection`. The initiator (typically the call originator, or the game peer with the smaller `AccountId`) creates the data channel explicitly; the acceptor receives it via `PeerConnection.Observer.onDataChannel(...)`.
- **`RealPeerChannelFactory`** — singleton factory with two creation paths:
  - `createSingleConnection(signaling, mediaConfiguration, scope, isInitiator, sessionId): PeerChannel` — used by calls (1:1).
  - `createGroupConnection(mediaConfiguration, scope): GroupPeerConnection` — used by games for sharing media tracks across many peers from one factory; `groupConnection.createPeer(...)` spawns child peer channels with their own scopes.

The transport itself is **stateless across reconnects**; on disconnect, the consumer is responsible for creating a new `PeerChannel`.

---

## Reserved use-case IDs

Some IDs are claimed by the transport layer or its built-in renegotiation flow. New use cases must not reuse:

- `"webrtc_renegotiation_internal_use_case"` — internal SDP/ICE renegotiation after the data channel is already open.
- `"webrtc_media_state_use_case"` — internal media-track state signals (camera/microphone on/off) used by calls.

Anything else is fair game. Convention: feature-prefix the ID (`"file_transfer_chunk"`).

---

## Existing consumers

### Calls (`feature/calls/impl/.../service/CallSessionManager.kt`)

- One `PeerChannel` per call session. Owned by `CallService` (foreground `Service`), so the call survives Activity recreation.
- Signaling: `RealExternalCallSignaling` posts/observes chat messages.
- Use case IDs: only the two reserved internal ones — calls don't add their own data-channel payloads (audio/video are media tracks, not data-channel messages).
- Reconnection: on `DataTransportState.Closed` or a `Failed` peer-connection state, the session ends. The user has to start a new call.

---

## Recipe — adding a new use case

For a new feature that wants to share an existing peer channel (the recommended path):

1. Pick a unique `UseCaseId`: `const val MY_USE_CASE = "feature_xxx_my_msg"`.
2. Define a `@Serializable` payload type. SCALE-encode to/from `ByteArray`.
3. Acquire the existing `PeerChannel` (e.g. from `CallSessionManager`).
4. Subscribe and send:
   ```kotlin
   peerChannel.dataTransport.subscribeMessages(MY_USE_CASE)
       .mapNotNull { BinaryScale.decodeFromByteArrayOrNull<MyMessage>(it) }
       .onEach(::handleMessage)
       .launchIn(scope)

   suspend fun emit(msg: MyMessage) {
       val data = BinaryScale.encodeToByteArray(MyMessage.serializer(), msg)
       peerChannel.dataTransport.send(MY_USE_CASE, data)
   }
   ```
5. Don't manage the transport lifecycle yourself — the owner of the `PeerChannel` does. When the owner closes, your flow simply stops emitting.

For a new feature that wants its **own** peer channel (rarer — only when no existing session is available):

1. Implement `PeerChannelSignaling` (define your SDP/ICE exchange protocol — chat? statement-store? something else?).
2. Inject `PeerChannelFactory` and call `createSingleConnection` (1:1) or `createGroupConnection` (N peers).
3. Call `channel.startConnection()` to begin the handshake; observe `dataTransport.state` for readiness.
4. Use the same subscribe/send pattern above.

---

## Lifecycle and threading

- **Scope-bound** — the `PeerChannel` is created with a `CoroutineScope`. When that scope cancels, the factory's `invokeOnCompletion` hook closes the WebRTC connection and frees EGL/factory resources. No explicit `dispose()` needed at consumer level beyond what the owner already does.
- **`awaitOpen()`** — suspend until the data channel reaches `Open`. Use before the first `send(...)` to avoid lost messages.
- **`isOpen()`** — synchronous snapshot; useful for fast-fail guards in UI handlers.
- **Background survival** — calls survive Activity recreation because `CallService` is a foreground `Service` that holds the channel. Games rely on a session scope that lives as long as the player is on the game screen; navigating away kills the channel.

---

## Error model

The transport does **not** retry on its own:

- Packet loss inside an ordered data channel → handled by WebRTC; transparent to the consumer.
- Connection drop → state moves through `Closing` to `Closed`; consumer chooses whether/how to recreate the channel.
- Peer gone → no automatic detection beyond state changes; consumers may layer their own heartbeats (calls watch `connectionState`; games subscribe to a `Reconnected` signal).

Fire-and-forget `send` has no acknowledgment in the transport layer. If the consumer needs request/response, build it on top with use-case-internal `requestId` correlation.

---

## Anti-patterns

| Anti-pattern | Severity | Fix |
|---|---|---|
| Reusing a reserved `UseCaseId` (`webrtc_renegotiation_internal_use_case`, `webrtc_media_state_use_case`) | blocking | pick a new feature-prefixed id |
| Bypassing `PeerChannelSignaling` and signaling from inside the transport | major | keep signaling in the consumer; transport stays generic |
| Reaching into the underlying WebRTC `DataChannel` directly | major | the transport's `subscribeMessages`/`send` are the only entry points |
| Calling `send` before `awaitOpen()` | major | use `awaitOpen()`, or use `isOpen()` as a guard |
| Mixing audio/video media tracks and data-channel messages through the same API | major | media is `MediaTrackProvider`; data is `DataTransport` — keep them separate |
| Sending payloads in raw text/JSON | major | always `@Serializable` + `BinaryScale` |
| Rolling your own reconnect inside the transport | major | the consumer chooses the policy; transport is one-shot |
| Spinning up two `PeerChannel`s when one would do | minor | reuse — share via a use-case ID |
| Hardcoding the use-case id literal in scattered call sites | minor | declare a `const val` in one place |

---

## Canonical examples

- Single 1:1 transport (calls): `RealCallSessionManager.startSession` → `peerChannelFactory.createSingleConnection(...)` → `channel.startConnection()`.
- Custom signaling: `RealExternalCallSignaling` (chat-based) and `SyncPeerChannelSignaling` (statement-store-based).

---

## North star

- **Multiple transports under one abstraction** — currently `DataTransport` is WebRTC-only. If a Bluetooth/NFC P2P channel is ever added, the `DataTransport` interface is the seam. Keep new consumers consuming the interface, not `RealDataTransport`.
- **Codec / track abstraction extends similarly** — `MediaTrackProvider` is the sibling abstraction for audio/video tracks. Treat the two as orthogonal: transport carries bytes, media-track-provider carries tracks.
