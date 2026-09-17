---
title: "Realtime Media Sessions for Products"
owner: "@replghost"
status: draft
---

# RFC 0029 — Realtime Media Sessions for Products

## Summary

A `Media` host API that lets a product run a one-to-one audio or video call
without implementing, embedding, or observing any realtime transport. The host
owns the peer connection, the capture devices, the codecs, the jitter buffers,
the audio route, and the on-screen video. The product supplies the signalling
channel it already has, positions the video the host draws, and receives
session state.

This is the capability a PolkaVM product needs. `RemotePermission::WebRtc`
exists today, but [RFC 0002] scopes it to the browser sandbox — "fetch
requests, WebSockets, WebRTC, and device permissions should be handled by the
Host's sandbox implementation" — and it is enforced by removing
`RTCPeerConnection` from a web realm. A PolkaVM product has no web realm and no
such object, so today it cannot place a call at all.

[RFC 0002]: 0002-permission-model.md

## Motivation

Chat is a PolkaVM product with an authenticated, end-to-end encrypted channel
to a contact and no way to turn that contact into a call. The transport is the
only missing piece: offer, answer, and candidate exchange can travel as
ordinary Chat messages over the channel that already carries text.

The alternative — a product shipping its own realtime stack — is not
acceptable on any axis:

- **It cannot work.** Capture devices, hardware codecs, the audio session, and
  the OS permission prompts are host-owned. A guest reaches none of them.
- **It should not work.** A media stack inside a product means microphone and
  camera frames inside a product. The device permission model exists to keep
  them out.
- **It would not be one stack.** Every product would carry its own ICE, its own
  echo cancellation, and its own bugs, and each would negotiate separately with
  the same user's OS.

The host already owns the equivalent surfaces for the browser case, including a
realtime engine and a TURN deployment. This RFC gives the same guarantee to
products that are not browsers, with an API shaped so that media never crosses
the product boundary.

## Approach

### Concepts

A **session** is one negotiated connection with one remote party. It is
identified by a `MediaSessionId` minted by the host, and it is bound to the
product that created it. A session carries at most one outbound audio track and
one outbound video track, and it reports the inbound tracks the remote party
offers.

A **signalling message** is an opaque, host-sealed byte string. The host emits
them; the product delivers them to the remote party over its own channel and
feeds received ones back. The product learns nothing from the bytes: no SDP, no
candidate, no address. The host seals each message to the session's peer using a
key pair it creates with the session, and publishes only the public half in the
first message. A product that tampers with, reorders, or replays a message
causes a session failure, not a disclosure.

A **media surface** is a rectangle, in the product's own surface coordinates,
where the host draws a track. The product chooses the rectangle, the track, the
corner radius, and the z-order relative to its own content. The host draws it,
and the frames never enter the product's address space.

### Service

New service, methods numbered from 196 upward (the next free block; `Locale`
holds 194).

```rust
#[wire(request_id = 196)]
async fn create_session(
    &self,
    cx: &CallContext,
    request: HostMediaCreateSessionRequest,
) -> Result<HostMediaCreateSessionResponse, CallError<HostMediaCreateSessionError>>;
```

`HostMediaCreateSessionRequest` names the direction (`Offer` or `Answer`), the
tracks the product wants to send (`audio: bool`, `video: bool`), and, for
`Answer`, the invitation it is answering. The response carries the
`MediaSessionId`. Creating a session starts nothing on the network: the first
signalling message is what does, which is why the user decision is taken here
rather than on the constructor.

```rust
#[wire(start_id = 198)]
async fn session_subscribe(
    &self,
    cx: &CallContext,
    request: HostMediaSessionSubscribeRequest,
) -> Subscription<HostMediaSessionSubscribeItem, CallError<HostMediaSessionSubscribeError>>;
```

One stream per session, carrying:

- `Signalling { message }` — hand this to the remote party.
- `State { state }` — `Negotiating`, `Connecting`, `Connected`, `Reconnecting`,
  `Ended { reason }`.
- `RemoteTracks { audio, video }` — what the peer is sending, so the product can
  lay out a video rectangle only when there is video.
- `Quality { level }` — a coarse `Good | Degraded | Poor`, never a bitrate,
  round-trip time, or address.
- `LocalDevices { microphone, camera }` — the host's view of what is actually
  live, which is not the product's request: a revoked OS permission or another
  app taking the camera changes this without the product asking.

```rust
#[wire(request_id = 202)]
async fn deliver_signalling(
    &self,
    cx: &CallContext,
    request: HostMediaDeliverSignallingRequest,
) -> Result<(), CallError<HostMediaDeliverSignallingError>>;

#[wire(request_id = 204)]
async fn set_local_tracks(
    &self,
    cx: &CallContext,
    request: HostMediaSetLocalTracksRequest,
) -> Result<(), CallError<HostMediaSetLocalTracksError>>;

#[wire(request_id = 206)]
async fn set_audio_route(
    &self,
    cx: &CallContext,
    request: HostMediaSetAudioRouteRequest,
) -> Result<(), CallError<HostMediaSetAudioRouteError>>;

#[wire(request_id = 208)]
async fn set_surfaces(
    &self,
    cx: &CallContext,
    request: HostMediaSetSurfacesRequest,
) -> Result<(), CallError<HostMediaSetSurfacesError>>;

#[wire(request_id = 210)]
async fn end_session(
    &self,
    cx: &CallContext,
    request: HostMediaEndSessionRequest,
) -> Result<(), CallError<HostMediaEndSessionError>>;
```

`set_local_tracks` mutes and unmutes the microphone and enables, disables, or
flips the camera. `set_audio_route` selects `Earpiece | Speaker | System`;
anything the OS routes on its own (a headset arriving) wins, and the change is
reported through `LocalDevices`. `set_surfaces` replaces the full set of
rectangles in one call, so a layout change is atomic. `end_session` is
idempotent and always allowed.

### Rendering

The host draws every track. The product sends rectangles; it never receives
frames.

This is the load-bearing decision, and the presentation stack already supports
it: the PolkaVM surface is a host-owned view inside a host-owned container, so a
video view is a sibling the host composites, positioned from the rectangles the
product supplied. Nothing about it is specific to one graphics profile, and no
new texture path is needed.

The rejected alternative is delivering decoded frames to the guest as textures.
It fails on three counts: `Tri2D` has no external-texture concept, every frame
would cross the guest boundary and be copied twice for no gain, and it would put
camera frames inside the product — exactly what the device permission model is
meant to prevent.

Consequences the product must accept: it cannot read, filter, screenshot, or
post-process call video, and a rectangle is a request that the host may clamp
to the visible surface.

### Permissions and consent

- Capture uses the existing device permissions, `Camera` and `Microphone`. No
  new device permission.
- A session needs an explicit user decision before the first signalling message
  leaves the device, because connecting reveals the user's address to the peer.
  `RemotePermission::WebRtc` already names this risk, but its
  remembered-grant-at-load semantics come from the browser case, where the gate
  is resolved before the realm exists. Sessions are per-call and need per-call
  intent, so `create_session` prompts and the answer is not remembered as a
  blanket grant. Whether that reuses the `WebRtc` variant or adds one is an open
  question below.
- A denied prompt fails `create_session`. A revoked permission mid-call ends the
  session and reports `Ended { reason: PermissionRevoked }`.
- The host may show its own always-visible call indicator. A product cannot
  suppress it, and cannot make a call invisible by placing every rectangle
  off-surface.

### Transport and addresses

The host owns the whole transport: candidate gathering, STUN, TURN credentials,
the selected pair, and every renegotiation. None of it is a product concern and
none of it is a product input.

The product therefore never learns either party's address. It sees only sealed
signalling blobs, session states, coarse `Quality`, and `RemoteTracks`; there is
no candidate list, no selected-pair report, no statistics object, and no relay
flag. That guarantee comes from sealing the signalling: a design that passed raw
SDP through the product would hand it the peer's host and server-reflexive
candidates, which is exactly the disclosure this API exists to prevent.

Address privacy *between the two devices* is a separate, host-chosen policy,
because ICE with a default transport policy still trades candidates end to end:

- **Relay-only** forces every packet through TURN. Neither device learns the
  other's address; the relay operator sees the flow, and latency and egress cost
  rise.
- **Direct-preferred** allows a peer-to-peer pair when NAT permits, so the two
  devices learn each other's addresses.

The host selects the policy and may differ per product or per network; the
product cannot request, detect, or override it. A host that serves a messaging
product where contacts are not mutually trusted with their locations should
default to relay-only.

### Host obligations

A host advertising this API must supply, from its own stack, the realtime
engine, capture pipeline, echo cancellation, audio session ownership, ICE with
STUN and TURN reachability (including short-lived TURN credentials it mints and
rotates without product involvement), and the compositing path that draws tracks
into the rectangles the product named. A host missing any of these answers
`Unsupported` rather than advertising partial support.

### What the product still owns

Signalling delivery, retries, and ordering; who is allowed to call whom;
identity and authentication of the peer; ringing, accept, and decline UI; call
history. Chat already has an authenticated encrypted channel, so it carries
signalling messages as ordinary messages and needs no signalling server.

### Incoming sessions, and why this is not the input modality

This API has no inbound listener. A session exists because a product called
`create_session`, so there is no host-level notion of "an incoming WebRTC call"
to route, and no ambiguity about which product a call belongs to: an invitation
is a message on a product's own channel, and the product that owns the channel
is the product that offers the call. For Chat that channel is the existing
authenticated conversation, and a call invitation is one more message kind in
it.

That is deliberately different from the input modality. The two look alike —
both end with the host handing work to a product that was not on screen — but
they differ where it matters:

| | Input modality | Incoming session |
| --- | --- | --- |
| Who starts it | the user, on this device | a remote party, over the network |
| Authorisation at dispatch | the user's own act | nobody; the peer chose the moment |
| Selection | the host asks which product handles this input | already decided: the channel's owner |
| Failure mode | the wrong product answers a query | an unwanted party makes the device ring |

Because the authorisation differs, the anti-abuse surface differs, and that is
why an incoming session must not be modelled as "input arriving from the
network". A modality-style registry answers *which product*, a question
channel ownership already answers here, and it answers nothing about *whether
this party may interrupt the user* — which is the only hard question on the
inbound path. That question belongs to the product: Chat knows whether the
sender is an accepted contact, because it already authenticates every message.

What a delivery-while-not-running path does need is orthogonal to this RFC and
shared with every other product that must react to a remote event: a wake
contract (which executable runs, with which capabilities, for how long, and how
often a peer may trigger it) layered on the existing notification path. That is
worth its own RFC, it is not media-specific, and this API works without it: a
call reaches a running product today, and gains background ringing when that
contract lands.

## Trade-offs

- One-to-one only. Group calls need a conference model — SFU addressing,
  per-participant tracks, speaker selection — that does not follow from this API
  and should not be retrofitted into it.
- No screen sharing, no recording, no virtual backgrounds, no custom codecs.
  Each needs its own consent story.
- Products cannot touch call media at all. This is the point, and it does
  foreclose product-side effects and overlays drawn from frames.
- Signalling is opaque, so a product cannot implement interoperability with a
  third-party SDP dialect. A host that wants that must expose it deliberately.
- Rectangles instead of frames means the host composites over product content.
  A product that wants video *behind* its own drawing gets that via z-order, but
  cannot sample it.

## Open questions

- Does session consent reuse `RemotePermission::WebRtc`, or does it need a
  variant with per-session semantics? Reuse keeps the permission catalogue
  small; a new variant avoids giving a browser-shaped remembered grant a second
  meaning.
- The API keeps relay-versus-direct invisible to the product, but a product that
  wants to warn a user about a relay's latency has no way to. Is the coarse
  `Quality` signal enough to carry that, or does the honest answer stay "no"?
- The wake contract is deferred to its own RFC. Does anything in this API have
  to change to accept it later, or does a background-delivered invitation reach
  `create_session` exactly as a foreground one does?
- Is `Quality` coarse enough to be safe, and useful enough to be worth sending?
