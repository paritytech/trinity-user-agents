# RFC: Host-mediated realtime media sessions

> Tracking-issue draft. File with labels `enhancement`, `rfc`. Fill in the RFC
> and implementation PR numbers once they exist.

**Source RFC:** #TBD · `docs/rfcs/realtime-media-sessions.md`
**Core implementation:** #TBD

## Problem

A user cannot make a call from a product. A messaging product can carry a
conversation with a contact, but it cannot offer to ring that contact, because
no Host service carries a live call.

The only way to do it today is for the product to drive WebRTC itself. That is
possible for a web product, since the browser engine is there, but it puts the
camera and microphone feed and both sides' network addresses inside product
code. Every other kind of product has no route at all.

Left there, each product that wants calls builds its own, users trust a
different implementation every time, and the Host has no way to show that a call
is running or to stop it.

## Goal

One `Media` service, implemented once, that any product can use to run an audio
or video call between two or more people, with the Host keeping everything
sensitive.

The Host owns the connection, the camera and microphone, the codecs, the audio
route, and the video on screen. The product says who is on the call over a
channel it already has, says where each participant's video goes, and is told
how the call is doing. A product never receives media, and never learns any
participant's network address.

Calls between more than two people are allowed, without this work prescribing
how. A small group call needs no new server, and none is in scope; large calls
would need infrastructure this work does not cover. Screen sharing and
recording are out. A product that is not running cannot yet be woken for an
incoming call; that is tracked separately.

## Requirements

- No media reaches a product: no frames, no tracks, no device handles.
- Signalling is sealed by the Host. A product learns no session detail and no
  participant's address.
- The Host draws each participant's video into rectangles the product names, at
  a depth the product chooses.
- Camera and microphone use the existing device permissions.
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
- The product keeps what it already owns: who may call whom, peer identity,
  ringing and decline, and call history.

## Service surface

| Method | Purpose |
| --- | --- |
| `create_session` | declare local tracks, take the user decision, mint a session |
| `add_participant` | invite a peer, or answer an invitation; returns a handle |
| `remove_participant` | drop one peer without ending the call |
| `deliver_signalling` | feed a received signalling message in |
| `session_subscribe` | signalling out, state, participants, tracks, quality, live devices |
| `set_local_tracks` | mute the microphone, enable, disable, or flip the camera |
| `set_audio_route` | earpiece, speaker, or system |
| `set_surfaces` | replace the whole set of video rectangles atomically |
| `end_session` | hang up on everyone; idempotent |

## Core implementation scope

- `Media` service definition, versioned types, and wire ids.
- Dispatcher and generated product clients.
- Session lifecycle, participants joining and leaving, ownership, consent, and
  teardown on permission withdrawal.
- Signalling sealing, including key rotation and revocation.
- Host engine binding: capture, echo cancellation, audio session, connectivity
  with Host-minted relay credentials, and video compositing.
- Conformance fixtures for the privacy guarantees, not only a working call.
- Reference product flow: invite, accept, decline, end.

## Implementation references

- RFC document: #TBD
- Background delivery to a product that is not running:
  `docs/rfcs/statement-routes-and-wake.md`

## Tasks

- [ ] RFC document body — #TBD
- [ ] RFC review and acceptance
- [ ] `Media` service definition, versioned types, wire ids
- [ ] Dispatcher and generated clients
- [ ] Session lifecycle, ownership, consent
- [ ] Signalling sealing, key rotation and revocation
- [ ] Video compositing contract, including clamping rules
- [ ] Audio-route and device-state contract
- [ ] Privacy conformance fixtures
- [ ] Product client wrappers
- [ ] Reference flow: invite, accept, decline, end
- [ ] Decide where the engine binding lives: a trait each Host implements, or
      shared Host code
- [ ] Decide whether an audio-only call may start without a visible surface
- [ ] Host adoption
  - [ ] Epoca
  - [ ] dotli-community
  - [ ] iOS
  - [ ] Android
