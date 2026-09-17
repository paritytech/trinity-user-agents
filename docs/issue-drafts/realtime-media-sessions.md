# RFC: Host-mediated realtime media sessions

> Tracking-issue draft. File with labels `enhancement`, `rfc`. Fill in the RFC
> and implementation PR numbers once they exist.

**Source RFC:** to follow
**Core implementation:** to follow

## Summary

A `Media` service that lets a product run an audio or video call, with screen
sharing, without implementing, embedding, or observing any realtime transport.
The Host owns the connections, the capture devices, the codecs, the audio route,
and the pictures on screen. The product carries opaque signalling over a channel
it already has, says where each participant's picture belongs, and is told how
the call is going.

This is the tracking issue for the implementation. The first implementation is
WebRTC, with the Host running the peer connections. Nothing a product sees says
so: it handles sealed messages and Host-minted handles, not transport detail.

## Requirements

- No media reaches a product: no frames, no tracks, no device handles.
- Signalling is sealed by the Host. A product learns no session detail and no
  participant's address.
- A call may be audio only. Camera and screen sharing are optional per
  participant and per direction.
- The Host draws each incoming picture — a participant's camera or their shared
  screen — into rectangles the product places, at a depth the product chooses.
- Screen sharing goes through the Host's own picker, so the user chooses what is
  shared and the product never names a window or a display.
- Camera and microphone use the existing device permissions.
- Devices belong to the Host: which microphone and camera are used, gain, echo
  cancellation, routing, and the in-call affordance for switching them. A
  product states what it wants sent and may express a preference, such as the
  front-facing camera or a speakerphone-style call; the Host or the OS may
  override it, and the product is told what is actually live and what kind of
  device the Host chose, so its call UI can show the truth. A product never
  learns a device name, model, or list.
- Starting a call, joining one, or answering an invitation each take an explicit
  user decision before anything reaches the network. Adding a further person to
  a call the user is already in does not ask again.
- A call ends when the user withdraws camera or microphone access.
- Participants are Host-minted handles. The Host is never told a product's own
  contact identities.
- Whether a call connects directly or through a relay is the Host's choice and
  invisible to the product.
- A Host that cannot provide the whole stack reports the service as unsupported
  rather than working partially.
- Calls between more than two people are allowed. A small group call needs no
  new server, and none is in scope; large calls would need infrastructure this
  work does not cover.
- Recording and product-chosen codecs are out of scope, as is waking a product
  that is not running for an incoming call.
- The product keeps what it already owns: who may call whom, peer identity,
  ringing and decline, and call history.

## Core implementation scope

- `Media` service definition, versioned types, and wire ids.
- Dispatcher and generated product clients.
- Session lifecycle, participants joining and leaving, ownership, consent, and
  teardown on permission withdrawal.
- Signalling sealing, including key rotation and revocation.
- Host WebRTC binding: peer connections, capture including the screen picker,
  echo cancellation, audio session, connectivity with Host-minted relay
  credentials, and compositing.
- Conformance fixtures for the privacy guarantees, not only a working call.
- Reference product flow: invite, accept, end.

## Implementation references

- RFC document: to follow
- Background delivery to a product that is not running: handled separately

## Target products

- DIM2, as an SPA
- Chat, as an SPA
- T3ams, for audio and video calling with screen sharing
- Meet, including screen sharing
- any other product needing peer-to-peer audio, video, or sharing

## Tasks

- [ ] RFC document body
- [ ] RFC review and acceptance
- [ ] `Media` service definition, versioned types, wire ids
- [ ] Dispatcher and generated clients
- [ ] Session lifecycle, ownership, consent
- [ ] Signalling sealing, key rotation and revocation
- [ ] Compositing contract for cameras and shared screens, including clamping
      rules
- [ ] Device ownership and live-state reporting contract
- [ ] Privacy conformance fixtures
- [ ] Product client wrappers
- [ ] Reference flow: invite, accept, end
- [ ] Decide where the WebRTC binding lives: a trait each Host implements, or
      shared Host code
- [ ] Decide whether a background worker may start an audio-only call before
      the product's surface is open
- [ ] Host adoption
  - [ ] Epoca
  - [ ] dotli-community
  - [ ] iOS
  - [ ] Android
