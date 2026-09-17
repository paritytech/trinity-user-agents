# RFC: Realtime Media Sessions for Products

> Tracking-issue draft, in the shape of #550. File as an issue with labels
> `enhancement`, `rfc`. Fill the RFC PR number into **Source RFC** and the
> implementation PR number into **Core implementation** once both exist.

**Source RFC:** #TBD · `docs/rfcs/0029-realtime-media-sessions.md` · @replghost
**Core implementation:** #TBD

## Problem

Products cannot place a call. `RemotePermission::WebRtc` exists, but RFC 0002
scopes it to the browser sandbox — "fetch requests, WebSockets, WebRTC, and
device permissions should be handled by the Host's sandbox implementation" — and
hosts enforce it by removing `RTCPeerConnection` from a web realm. A PolkaVM
product has no realm and no such object, and none of the 18 product-facing
TrUAPI services carries media, so the capability is absent rather than
restricted.

A product cannot close that gap itself. Capture devices, hardware codecs, the
audio session, and the OS permission prompts are host-owned, and a media stack
inside a product would put microphone and camera frames inside a product —
precisely what the device permission model exists to prevent.

## Goal

Define one host-neutral `Media` service through which a product runs a
one-to-one audio or video call without implementing, embedding, or observing any
realtime transport, and implement it once so every host behaves identically.

The host owns the peer connection, capture, codecs, ICE with STUN and TURN, the
audio route, and all on-screen video. The product carries signalling over a
channel it already has, positions the rectangles the host draws, and receives
session state.

## Requirements

- Media never crosses the product boundary. No frames, no tracks, no device
  handles.
- Signalling is opaque and host-sealed. A product learns no SDP, no candidate,
  and neither party's address.
- Video is composited by the host into rectangles named in the product's own
  surface coordinates, with product-chosen z-order. No external-texture or
  frame-delivery path.
- Capture reuses the existing `Camera` and `Microphone` device permissions. No
  new device permission.
- A session requires an explicit user decision before the first signalling
  message leaves the device, and ends when a required permission is revoked.
- Relay-versus-direct ICE policy is host-chosen and invisible to the product; a
  messaging host defaults to relay-only so neither device learns the other's
  address.
- One-to-one only. Group calls, screen sharing, recording, and custom codecs are
  out of scope.
- A host that cannot supply the full stack answers `Unsupported` rather than
  advertising partial support.
- The product keeps signalling delivery, peer authentication, call policy,
  ringing and decline UI, and call history.

## Service surface

New service, methods numbered from 196 upward (`Locale` holds 194):

| Method | Kind | Purpose |
| --- | --- | --- |
| `create_session` | request | mint a session, prompt for consent, declare local tracks |
| `session_subscribe` | subscription | signalling out, state, remote tracks, quality, live device state |
| `deliver_signalling` | request | feed a received signalling message in |
| `set_local_tracks` | request | mute/unmute microphone, enable/disable/flip camera |
| `set_audio_route` | request | earpiece, speaker, or system routing |
| `set_surfaces` | request | replace the full set of video rectangles atomically |
| `end_session` | request | hang up; idempotent and always allowed |

## Core implementation scope

- `Media` trait, versioned request/response/item/error types, and wire ids in
  `truapi`.
- Dispatcher wiring and generated clients, including the Rust product client.
- Session lifecycle and per-product session ownership in `truapi-server`,
  including consent, permission-revocation teardown, and `Unsupported` on hosts
  without a stack.
- Signalling sealing: per-session key pair, public half published in the first
  message, tamper and replay causing session failure rather than disclosure.
- Host-side engine binding: capture pipeline, echo cancellation, audio-session
  ownership, ICE with host-minted rotating TURN credentials, and the compositing
  path that draws tracks into product-named rectangles.
- Conformance fixtures asserting the privacy properties, not just the happy
  path: no address-bearing field on any emitted item, sealed signalling rejected
  on tamper, revocation ending the session.

## Non-goals

Group calls and any SFU addressing; screen sharing; recording; virtual
backgrounds; product-selected codecs; third-party SDP interoperability; the
background wake contract (see below).

## Relationship to the input modality and the wake contract

An incoming call is not an input-modality dispatch. The input modality answers
*which product handles input the user just produced on this device*; an
invitation arrives from a remote party on a product's own channel, so the
product is already determined by channel ownership, and the open question is
*whether this party may interrupt the user* — which the product answers, because
it authenticates the channel.

Delivering an invitation to a product that is not running needs a wake contract:
which executable runs, with which capabilities, for how long, and how often a
remote party may trigger it. That is shared with every product reacting to a
remote event, is not media-specific, and belongs in its own RFC. This API works
without it — a call reaches a running product, and gains background ringing when
the contract lands.

## Tasks

- [ ] RFC document body — #TBD
- [ ] RFC review and acceptance
- [ ] `Media` trait, versioned types, and wire-id allocation
- [ ] Dispatcher and generated client surfaces
- [ ] Session lifecycle, ownership, and consent in `truapi-server`
- [ ] Signalling sealing and its tamper/replay tests
- [ ] Compositing contract for host-drawn tracks, including clamping rules
- [ ] Audio-route and device-state reporting contract
- [ ] Privacy conformance fixtures
- [ ] PolkaVM product client wrappers
- [ ] Reference product flow: call invitation, accept, decline, end over an
      existing authenticated channel
- [ ] Host adoption
  - [ ] Epoca
  - [ ] dotli-community
  - [ ] iOS
  - [ ] Android
- [ ] Follow-up RFC: background wake contract for remote-initiated delivery
