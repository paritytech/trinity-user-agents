---
title: "Realtime Media Sessions for Products"
owner: "@replghost"
status: draft
---

# RFC — Realtime Media Sessions for Products

## Summary

A `Media` service that lets a product run an audio or video call, with screen
sharing, without implementing, embedding, or observing any realtime transport.
The host owns the connections, the capture devices, the codecs, the audio route,
and the pictures on screen. The product carries opaque signalling over a channel
it already has, says where each participant's picture belongs, and is told how
the call is going.

The first implementation is WebRTC, with the host running the peer connections.
Nothing a product sees says so: it handles sealed messages and host-minted
handles, not transport detail.

## Motivation

A product has no way to make a call without running the WebRTC stack in its own
code. No host service carries real-time audio or video, so a web product must
drive `RTCPeerConnection` itself — `RemotePermission::WebRtc` ungates it — and
every other kind of product cannot make a call at all.

Running it in product code is the wrong place for it. The camera and microphone
feed and every participant's network address pass through the product, defeating
the device permissions that exist to keep them out. The capture devices, the
hardware codecs and the audio session are host-owned, so the product is fighting
for things it does not control. And each product that tries ends up with its own
stack, its own bugs, and no single place for the user to see or stop a call.

## Approach

### Concepts

A **session** is one call. It has a host-minted `MediaSessionId`, belongs to the
product that created it, and holds one or more remote **participants**. A
participant is a host-minted handle, meaningless outside its session: enough to
say which peer a signalling message is for, whose tracks arrived, and whose
picture a rectangle draws. The host is told no product-side identity, and the
product keeps its own mapping from handle to contact.

A **signalling message** is an opaque, host-sealed byte string, addressed to one
participant. The host emits them, the product delivers them over its own
channel, and feeds received ones back. The product learns nothing from the
bytes: no session description, no candidate, no address.

An **invitation** is the first signalling message, and it carries no addresses:
the host puts its session's public key and negotiation terms in it, and nothing
that locates the device. Candidates come later, sealed to the key pair the two
hosts agree from the invitation and its answer. Both key pairs are per session,
so there is no long-lived identity to publish, rotate, or correlate. Tampering
or replay fails the session.

A **surface rectangle** places one incoming picture — a participant's camera or
their shared screen — in the coordinates of whatever surface the product already
draws into. The product chooses the rectangle, the corner radius, and the depth
relative to its own content. A call with no pictures needs none: audio-only
calls are ordinary, camera and screen are optional per participant and per
direction, and a product places a rectangle only for a picture actually
arriving.

### Service

`create_session` says which local tracks to send — microphone, camera, screen,
or none of them — prompts the user, and returns the session id. It connects
nothing on its own.

`add_participant` adds one peer. Given an invitation the product received, the
host answers it; given none, the host produces an invitation for the product to
deliver. Either way it returns a participant handle, so offering and answering
are per participant rather than per call — in a group call a product may be
answering one peer while inviting another. `remove_participant` drops one peer
without ending the call.

`session_subscribe` streams everything the product needs to know:

- signalling to deliver, per participant;
- session state — negotiating, connecting, connected, reconnecting, ended;
- participants joining and leaving, and which tracks each is sending;
- a coarse quality level, never a bitrate, round-trip time, or address;
- whether the microphone and each picture are actually live, and what the host
  chose: earpiece, speaker, or a headset; front or rear camera. None of it is
  what the product asked for — a withdrawn permission, another app taking the
  camera, or the OS moving the route changes it unprompted — so a call UI can
  show the truth.

`deliver_signalling` feeds a received message in. `set_local_tracks` states what
the product wants sent — microphone on or muted, camera on or off, screen shared
or not — and may express a preference such as the front-facing camera or a
speakerphone-style call. `set_surfaces` replaces the whole rectangle set at
once, so a layout change is atomic. `end_session` hangs up on everyone, is
idempotent, and is always allowed.

Devices belong to the host. It chooses which microphone and camera to use, owns
gain, echo cancellation, and routing, and owns whatever in-call affordance lets
the user switch them. A product preference is a preference: the host may ignore
it, the OS may override it the moment a headset appears, and the stream reports
what is live rather than what was asked for. The product is told the kind of
device in use, which is what a call UI needs, and never a device name, model, or
list: those would be a fingerprinting surface for no gain.

### The host draws the pictures

The product sends rectangles and never receives frames. Every host already
composites the product's own surface — a canvas, a web view, a native view — so
a picture layer is a sibling it positions from the rectangles the product gave
it. Nothing here depends on how the product renders.

The alternative, handing decoded frames to the product as textures, is rejected:
it puts camera output inside the product, and it copies every frame across the
product boundary for nothing.

The product therefore cannot read, filter, capture, or post-process an incoming
picture, and a rectangle is a request the host may clamp to what is actually
visible.

### Consent and addresses

Microphone and camera capture use the existing `Camera` and `Microphone`
permissions. Screen capture is not a device permission: the host runs its own
picker, so the user chooses what is shared, the product never names a window or
a display, and the OS prompt or broadcast flow the platform requires stays the
host's business. Beyond that:

- Connecting exposes the user's address to the other participants, so a call
  needs an explicit decision before the first signalling message leaves the
  device. Starting a call, joining one, and answering an invitation all go
  through `create_session`, so all three take that decision; inviting a further
  peer into a call the user is already in does not ask again.
- Withdrawing camera or microphone access ends the session. Ending a screen
  share stops that track and leaves the call running.
- The host may show its own call indicator, which a product cannot suppress —
  including by placing every rectangle out of view.

The host owns the transport end to end: candidate gathering, relay credentials it
mints and rotates, the selected path, and every renegotiation. The product never
learns any participant's address, because it only ever sees sealed blobs and
coarse state. Whether a call runs directly or through a relay is a host policy
the product cannot request, detect, or override; a host whose users should not
reveal their location to their contacts relays by default.

Group calls are allowed and deliberately unspecified: participants are a set,
and how the host connects them is its own business. A small call needs no new
server, and none is proposed here.

### Incoming calls

There is no inbound listener. A session exists because a product created one, so
there is nothing for a host to route and no question of which product a call
belongs to: an invitation is a message on the product's own channel, and the
channel's owner is the product that offers the call. Whether a particular sender
may interrupt the user is a product decision, made on material the product
already authenticates — not something a host registry can answer.

A product's background worker is the right place to notice an invitation, and it
is inside the same boundary as the rest of the product: the invitation carries no
addresses, so the worker can store and forward it and still learn nothing, and no
call hands it a candidate or a device identifier. It learns only what it knew
already — which of its own contacts is calling.

Waking a product that is not running is out of scope here. It is a
media-neutral problem — every product that reacts to a remote event needs the
same thing — and will be dealt with separately. Until then, a call reaches a
product that is already open.

## Trade-offs

- Products cannot touch call media. That is the point, and it forecloses
  product-drawn effects and overlays on a call picture.
- Opaque signalling means no interoperability with a third-party dialect.
- Recording and product-chosen codecs are out; each needs its own consent
  story.
- A host must supply the whole stack — engine, capture, echo cancellation, audio
  session, connectivity, compositing — or report the service unsupported. There
  is no partial mode.
- Group calls work without a server, but not at arbitrary size. A host that
  wants large calls needs infrastructure this RFC does not describe.
- Sealing protects addresses from a product that forwards faithfully. A product
  that substitutes its own key when relaying an invitation could read what
  follows, and no design placing the product on the signalling path prevents
  that: the product is the channel.

## Open questions

- Does per-call consent reuse `RemotePermission::WebRtc`, or need its own
  variant? Reuse keeps the catalogue small but gives a browser-shaped remembered
  grant a second meaning.
- May a call start without a visible surface, so audio can connect while the
  product is still opening? That would let a background worker create an
  audio-only session.
- Is a coarse quality level worth sending at all, given a product cannot act on
  the reason behind it?
- Should a host be able to prove to another host that a key came from it, so a
  substituted key fails rather than succeeds silently? That needs a trust root
  the product cannot touch, which no channel here provides.
