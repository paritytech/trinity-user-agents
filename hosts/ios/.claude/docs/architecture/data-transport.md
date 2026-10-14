# Data Transport (WebRTC)

## Overview

Peer-to-peer data transport using WebRTC for voice/video calls and real-time data channels. Used by chat calls and device sync.

## Key Components

### External
- **WebRTC** — WebRTC framework

### Modules
- **ChatCall** — voice/video call UI and management

### Services
- `callCoordinator` (ServiceCoordinator) — call lifecycle management
- `audioSessionManager` — audio routing

### Related
- **HandoffService** (`Packages/HandoffService/`) — handoff/continuity
- Push notifications via PushKit (VoIP) for incoming calls

## Architecture

```
PeerChannelSignaling (negotiation)
        ↓
InitiatorConnection / AcceptorConnection (WebRTC lifecycle)
        ↓
DataTransport (message send/receive, state)
        ↓
UseCaseId multiplexing (multiple concerns per connection)
```

## Hard Rules

1. **Consumer chooses reconnect policy, not transport** — the transport layer exposes state; reconnection logic belongs to the feature using it
2. **UseCaseId multiplexing** — multiple features can share a WebRTC connection via use case identifiers
3. **Exclude `disconnected` from terminal states** — a disconnected state is not necessarily terminal for the connection
4. **Detect call disconnect robustly** — use multiple signals, not just WebRTC state
5. **Rename config for actual scope** — if a config is used for data-channel-only cases too, name it accordingly (e.g., `P2PConnectionConfigFactory`) (PR review)

## Connection Lifecycle

1. PushKit/APNs notification for incoming call
2. Signaling exchange via statement store
3. WebRTC connection established (ICE/DTLS)
4. Media streams or data channels active
5. Connection teardown on call end

## Seams

| Seam                    | Where                          | When to touch                       |
|-------------------------|--------------------------------|-------------------------------------|
| WebRTC config           | Connection factory             | ICE/TURN server changes             |
| Signaling               | Statement store integration    | Signaling protocol changes          |
| Call UI                 | `Modules/ChatCall/`           | Call screen UI changes              |
| Audio routing           | `audioSessionManager`          | Audio session category changes      |
| Data channels           | DataTransport                  | New real-time data features         |
