# Device Sync

## Overview

Device sync maintains one peer lifecycle per supported linked device. Each peer runs independently, while WebRTC
connection generations for the same peer are strictly sequential: the current generation is closed and awaited before
its replacement starts. `ServiceCoordinator` configures the service automatically after sign-in setup and throttles it
with the rest of the application services.

## Key Components

- `DeviceSyncService` — reconciles linked devices with one peer engine per supported remote device
- `DeviceSyncPeerEngine` — exposes the async lifecycle for one remote device
- `DeviceSyncPeerEngineContextFactory` — assembles the per-peer context dependencies
- `DeviceSyncPeerEngineContext` — actor that owns the signaling session, current flow/exchange, retry policy, and disposal
- `DeviceSyncPeerComponentFactory` — creates the signaling session and replaceable flow/exchange pairs
- `DeviceSyncSessionFactory` — creates the MessageExchange transport and stable signaling session
- `DeviceSyncPeerSessionDelegate` — adapts MessageExchange callbacks into an ordered envelope stream
- `DeviceSyncExchange` — exchanges updates and acknowledgements and persists per-peer sync progress
- `DeviceSyncSignalingSession` — translates device signaling envelopes to WebRTC signals and survives flow replacement
- `DeviceSyncPeerConnectionFlow` — owns one WebRTC data-connection generation and its connected resources
- `DeviceSyncMessageTransport` — carries encrypted signaling envelopes through Message Exchange Kit

## Peer Lifecycle

```
DeviceSyncService
        ↓ one engine per remote device
DeviceSyncPeerEngine
        ↓ context lifecycle calls
DeviceSyncPeerEngineContext
        ↑ ordered envelopes from DeviceSyncPeerSessionDelegate
        ├── DeviceSyncSignalingSession (stable)
        ├── current DeviceSyncPeerConnectionFlow → DataConnectionCreator → MultiplexedDataChannel
        └── current DeviceSyncExchange
```

## Hard Rules

1. **One peer engine per remote device** — `DeviceSyncService` owns device-set reconciliation. Removing a device disposes
   its engine; adding it again creates a fresh engine.
2. **Peers run independently** — a slow or offline device must not delay another device's connection lifecycle.
3. **Context owns peer lifecycle state** — `DeviceSyncPeerEngineContext` is the synchronization point for the stable
   signaling session, current flow/exchange, ordered envelopes, retry delay, replacement ordering, and disposal.
4. **Connection generations are sequential per peer** — the current exchange and WebRTC resources are closed and awaited
   before the next generation is created.
5. **Dispose is terminal** — a disposed engine cannot schedule a retry or create another generation.
6. **Stale failures are ignored** — only the currently registered connection ID may trigger replacement.
7. **WebRTC resources have explicit ownership** — connected data-channel and peer-connection wrappers are retained and
   explicitly closed; their deinitializers are not the normal cleanup path.
8. **Reconnect policy stays above transport creators** — `DataConnectionCreator` remains a cancelable primitive and does
   not decide when a peer should reconnect.
9. **Durable progress crosses flows** — a replacement exchange reloads the device's checkpoint and accepted offer
   ID before it starts.
10. **Service shutdown awaits peer disposal** — throttle removes engines from reconciliation state and waits for every
    engine to close.
11. **Automatic sync is the only connection owner** — the linked-device subscription creates peer engines; there is no
    separate one-shot/manual WebRTC generation.
12. **Reconnect identity is durable** — the acceptor persists a validated offer before exposing it to WebRTC; the
    initiator persists the matching validated answer before exposing it. The last accepted offer ID is replaced only by
    a newer accepted negotiation and is not cleared when a reconnect marker arrives. Persistence and the context's
    in-memory reconnect fallback are updated by the same context-owned operation, in that order.
13. **Reconnect markers are scoped** — a reconnect marker is accepted only when it matches the signaling session's
    last accepted offer ID. Before every replacement attempt, the context sends a marker for the last accepted or persisted ID.
14. **Bare offers do not replace an active generation** — offer IDs are UUIDs and carry no ordering information. While
    the acceptor has an active negotiation, different offers are conflated into one latest pending offer. The pending
    offer is consumed only after a valid reconnect marker or terminal flow failure starts an ordered replacement.
15. **ACK timeout retries the sync round, not WebRTC** — an unacknowledged snapshot is discarded in memory and collected
    again from the durable checkpoint on the same connected flow.
16. **Empty scans advance progress** — a successful scan with no entities still persists its collection time, preventing
    later scans from repeatedly traversing the same interval.
17. **Flow events require no replay buffer** — the exchange creates the update, acknowledgement, and state iterators
    before the engine starts the flow. Iterator creation synchronously registers all three passthrough subscriptions.
18. **Incoming entity application is best effort** — an entity that cannot be applied is logged and skipped without
    blocking the remaining entities. The update is still acknowledged after all entities are processed, so skipped
    entities are not retried by the peer; only failure to send the acknowledgement fails the exchange.

## Connection Flow

1. `DeviceSyncService` receives the linked-device snapshot.
2. It creates and starts an engine for each newly supported remote device without waiting for WebRTC connection.
3. The context creates one signaling session through `DeviceSyncPeerComponentFactory` and keeps it across WebRTC retries.
4. `DeviceSyncPeerSessionDelegate` yields callback batches to the context in arrival order.
5. Before each attempt, the context sends a reconnect marker for the last accepted or persisted offer ID. It then creates
   a flow and exchange, installs the exchange's update, acknowledgement, and state iterators, and only then starts the flow.
   The live `.connected` event triggers the initial push from the durable checkpoint.
6. Normal signaling batches are delivered to the signaling session.
7. A valid reconnect batch makes the context close and await the exchange and flow, reset signaling state, then create a
   replacement flow/exchange and deliver the envelopes that followed the reconnect marker.
8. A different offer received by the acceptor is retained as the latest pending offer without replacing the active
   generation. The next marker- or failure-driven replacement consumes it; duplicate active offers are ignored.
9. A retryable flow/exchange failure follows the same ordered replacement path after cancellation-aware backoff.
10. A signaling-session creation failure retries the complete session; ordinary WebRTC failures retain the session.
11. Foreground and background transitions do not replace healthy flows. Transient ICE `disconnected` state remains
    owned by WebRTC and may recover; only terminal data-channel or ICE failures use the failure-driven replacement path.
12. Device removal or service throttle finishes the delegate stream and awaits exchange, flow, signaling, and transport
    cleanup.
13. An ACK timeout recollects and resends from the last acknowledged checkpoint without replacing the flow. A successful
    empty collection advances that checkpoint without putting an empty update on the wire.

## Data-Channel Flow Control

`AsyncDataChannelWrapper` owns one FIFO send drainer. Every send attempt reads native `bufferedAmount` and conditionally
calls `sendData` in one peer-connection serial-queue operation. Writers wait outside that queue above the 4 MiB high
watermark and resume after the buffer reaches the 1 MiB low watermark. A message larger than the high watermark waits
for an empty native buffer and sends alone.

Buffered-amount callbacks are coalesced wakeups, not accounting. Late, duplicate, or reordered wakeups only cause a
redundant authoritative read. Cancellation and close also wake the drainer. A native send failure fails only that request
and preserves the previous retry semantics for later sends; only lifecycle closure terminates queued and future requests.

The wrapper owns its delegate consumer, inbound-message producer, and outbound drainer tasks. Public close transitions
the serialized lifecycle first, finishes the outbox, wakes and awaits the drainer, closes WebRTC, releases the inbound
producer, and awaits every owned task. Waiting for buffer capacity inside the peer serial queue is prohibited because the
drainer's authoritative attempt and lifecycle events need that same queue.

Device-sync entity chunking still controls individual update size; an ACK-based in-flight update window can be added
separately if snapshot memory needs a tighter bound.

## Seams

| Seam | Where | When to touch |
| --- | --- | --- |
| Peer membership | `DeviceSyncService` | Device add/remove and service configuration |
| Per-peer lifecycle | `DeviceSyncPeerEngineContext` | Ordered signaling, retry, replacement, recovery, disposal |
| Sync protocol | `DeviceSyncExchange` | Updates, acknowledgements, checkpoints, conflict handling |
| WebRTC generation | `DeviceSyncPeerConnectionFlow` | Data-channel setup, monitoring, and resource cleanup |
| Signaling protocol | `DeviceSyncSignalingSession` | Offer IDs, reconnect markers, candidate filtering |
| Signaling transport | `DeviceSyncMessageTransport` | Message Exchange setup, delivery, and transport errors |

## Testing

Device-sync tests synchronize on recorded lifecycle events. Retry and ACK-timeout tests inject a manually advanced sleep
operation; they must not assert against elapsed wall-clock time or poll with short sleeps. Exchange tests also inject
their storage dependencies so they are safe to run concurrently without app-group entitlements.
