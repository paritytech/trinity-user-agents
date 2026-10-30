---
title: "Realtime Media Sessions for Products"
owner: "@replghost"
status: draft
---

# RFC — Realtime Media Sessions for Products

## Summary

A `Media` service that lets a product run an audio or video call, with screen
sharing, without running a realtime stack in its own code. The host owns the
connections, the signalling, the capture devices, the codecs, the audio route,
and the pictures on screen. The product names who is on the call, says where
each picture belongs, and is told how the call is going.

The first implementation is WebRTC, with the host running the peer connections.
Nothing a product sees says so: it names peers and host-minted handles, not
transport detail.

## Motivation

A product has no way to make a call without running the WebRTC stack in its own
code. No host service carries real-time audio or video, so a web product must
drive `RTCPeerConnection` itself, and every other kind of product cannot make a
call at all.

Running it in product code is the wrong place for it. The camera and microphone
feed and every participant's network address pass through the product,
defeating the device permissions that exist to keep them out. The capture
devices, the hardware codecs and the audio session are host-owned, so the
product is fighting for things it does not control. And each product that tries
ends up with its own stack, its own bugs, and no single place for the user to
see or stop a call.

## Approach

### Concepts

A **session** is one call. It has a host-minted `MediaSessionId`, belongs to the
product that created it, and holds one or more remote **participants**. A
participant is a host-minted handle: enough to say whose tracks arrived and
whose picture a rectangle draws.

**Signalling belongs to the host.** The product names a peer; the host offers,
answers, and exchanges candidates over the statement store, and the product
never carries, stores, or sees any of it. No session description, no candidate,
and no address reaches product code, so there is nothing to seal and no key to
distribute. The host owns that subscription, so calling needs no statement
permission of its own.

A **picture rectangle** places one incoming picture — a participant's camera or
their shared screen — in the coordinates of the surface the product draws into,
with a corner radius and a depth relative to the product's own content.
Audio-only calls are ordinary and need none; camera and screen are optional per
participant and per direction. A web product usually has a placeholder element
rather than coordinates, so an SDK may offer `attach(track, element)` and keep
the rectangle updated as layout changes — sugar over the same wire contract, not
a second one.

### Service

`create_session` says which local tracks to send — microphone, camera, screen,
or none of them — and returns the session id. It connects nothing on its own,
and needs no visible surface: audio connects while the product is still opening,
so a background worker can answer a call and the picture arrives when there is
somewhere to draw it.

`add_participant` names one peer and returns a participant handle. The host
reaches that peer over the statement store; answering an incoming call names the
peer the same way. Offering and answering are therefore per participant rather
than per call, so in a group call a product may be answering one peer while
inviting another. `remove_participant` drops one peer without ending the call.

`session_subscribe` streams everything the product needs to know:

- session state — negotiating, connecting, connected, reconnecting, ended;
- participants joining and leaving, and which tracks each is sending;
- an incoming call the host has been offered, for the product to accept or
  refuse;
- whether the microphone and each picture are actually live, and what the host
  chose: earpiece, speaker, or a headset; front or rear camera. None of that is
  what the product asked for — a withdrawn permission, another app taking the
  camera, or the OS moving the route changes it unprompted — so a call UI can
  show the truth.

`set_local_tracks` states what the product wants sent — microphone on or muted,
camera on or off, screen shared or not — and may express a preference such as
the front-facing camera or a speakerphone-style call. `set_surfaces` replaces
the whole rectangle set at once, so a layout change is atomic. `end_session`
hangs up on everyone, is idempotent, and is always allowed.

Devices belong to the host: which microphone and camera, gain, echo
cancellation, routing, and the in-call affordance for switching them. A product
preference stays a preference — the host may ignore it and the OS may override
it the moment a headset appears. The product is told the kind of device in use,
which is what a call UI needs, and never a name, model, or list. Nothing
reports bitrates, round-trip times, candidates, or addresses: a product cannot
act on them, and candidate pairs carry exactly what this design keeps out of
product code.

### The host draws the pictures

The product sends rectangles and never receives frames. Every host already
composites the product's own surface — a canvas, a web view, a native view — so
a picture layer is a sibling it positions from the rectangles it was given, and
nothing here depends on how the product renders. The alternative, handing
decoded frames to the product as textures, is rejected: it puts camera output
inside the product and copies every frame across the boundary for nothing.

So a product cannot read, filter, capture, or post-process an incoming picture,
and a rectangle is a request the host may clamp to what is actually visible.

### Permission and control

Calling is its own permission, asked once and remembered, like every other in
[RFC 0002](0002-permission-model.md). It is not `RemotePermission::WebRtc`,
whose grant is resolved at load time so a browser realm can be locked down
before page script runs; that timing is browser-specific and says nothing about
a product placing a call. A host may offer to allow a single call instead of
remembering, which is prompt behaviour rather than a second kind of grant. Microphone and camera capture keep using the existing `Camera` and
`Microphone` permissions; screen capture is not a device permission, because the
host runs its own picker, so the user chooses what is shared and the product
never names a window or a display.

A remembered grant must not become an invisible call, so control sits in the
host rather than in a prompt:

- The host shows its own in-call indicator, which a product cannot suppress —
  including by placing every rectangle out of view — and from which the user can
  end the call.
- Withdrawing camera or microphone access ends the session. Ending a screen
  share stops that track and leaves the call running.
- Revoking the calling permission ends any call the product is in.

### Addresses

The host owns the transport end to end: candidate gathering, relay credentials
it mints and rotates, the selected path, and every renegotiation. Whether a call
runs directly or through a relay is a host policy the product cannot request,
detect, or override. A host whose users should not reveal their location to
their contacts relays everything, which costs latency and egress and is the
right default for a messaging product.

Group calls are allowed, and how a host connects them is its own business.
A host must carry at least six participants, which is what a shared game needs
and what a mesh does comfortably; beyond that it may refuse, and a refusal is
reported so a product can grey out the control rather than fail mid-call. No new
server is required at that size, and none is proposed here.

### Incoming calls

An incoming call arrives on the host's statement subscription and reaches the
product through `session_subscribe`, naming the peer. There is no routing
question: the host knows which product a call is for, because the signalling is
addressed to that product's channel. Whether a particular caller may interrupt the user is still a
product decision — the product knows whether that peer is an accepted contact —
so the product accepts or refuses.

Waking a product that is not running is out of scope here. It is a
media-neutral problem — every product that reacts to a remote event needs the
same thing — and will be dealt with separately. Until then, a call reaches a
product that is already open.

## Trade-offs

- Products cannot touch call media. That is the point, and it forecloses
  product-drawn effects and overlays on a call picture.
- Signalling over the statement store ties calling to it, and a product cannot
  interoperate with an outside endpoint that expects to exchange session
  descriptions itself.
- Nothing reports call quality, so a product cannot tell a user their
  connection is poor. Adding a coarse signal later breaks nothing.
- Recording and product-chosen codecs are out; each needs its own consent
  story.
- Group calls work without a server, but not at arbitrary size. A host that
  wants large calls needs infrastructure this RFC does not describe.
- A host must supply the whole stack — engine, signalling, capture, echo
  cancellation, audio session, connectivity, compositing — or report the
  service unsupported. There is no partial mode, so a host adds calling for
  every product at once or not at all.
