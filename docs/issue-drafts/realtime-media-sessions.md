# RFC: Host-mediated realtime media sessions

> Tracking-issue draft. File with labels `enhancement`, `rfc`. Fill in the RFC
> and implementation PR numbers once they exist.

**Source RFC:** #TBD · `docs/rfcs/0029-realtime-media-sessions.md`
**Core implementation:** #TBD

## Problem

Products cannot offer calls. No Host API carries audio or video, so a product
that wants a voice or video call has to bring its own realtime stack — which
only web products can do, and only by handling microphone and camera frames,
plus every participant's network address, inside product code. Every product
would ship a different stack, and the user would have no single place to see or
stop a call.

## Goal

One `Media` service, implemented once, that any product can use to run an audio
or video call between two or more people, with the Host keeping everything
sensitive.

The Host owns the connection, the camera and microphone, the codecs, the audio
route, and the video on screen. The product says who is on the call over a
channel it already has, says where each participant's video goes, and is told
how the call is doing. A product never receives media, and never learns any
participant's network address.

Screen sharing and recording are not part of this work. A product that is not
running cannot yet be woken for an incoming call; that is tracked separately.

## Requirements

- No media reaches a product: no frames, no tracks, no device handles.
- Signalling is sealed by the Host. A product learns no session detail and no
  participant's address.
- The Host draws each participant's video into rectangles the product names, at
  a depth the product chooses.
- Camera and microphone use the existing device permissions.
- A call needs an explicit user decision before it reaches the network, and ends
  when the user withdraws camera or microphone access.
- How a call is carried — directly, through a relay, or through a Host-operated
  conference server — is the Host's choice and invisible to the product.
- A Host that cannot provide the whole stack reports the service as unsupported
  rather than working partially.
- The product keeps what it already owns: who may call whom, peer identity,
  ringing and decline, and call history.

## Core implementation scope

- `Media` service definition, versioned types, and wire ids.
- Dispatcher and generated product clients.
- Session lifecycle, participants joining and leaving, ownership, consent, and
  teardown on permission withdrawal.
- Signalling sealing, including key rotation and revocation.
- Host engine binding: capture, echo cancellation, audio session, connectivity
  with Host-minted relay credentials, conference topology, and video
  compositing.
- Conformance fixtures for the privacy guarantees, not only a working call.
- Reference product flow: invite, accept, decline, end.

## Implementation references

- RFC document: #TBD
- Background delivery to a product that is not running:
  `docs/rfcs/0030-statement-routes-and-wake.md`

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
