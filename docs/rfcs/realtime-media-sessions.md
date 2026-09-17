---
title: "Realtime Media Sessions for Products"
owner: "@replghost"
status: draft
---

# RFC — Realtime Media Sessions for Products

## Summary

A `Media` service that lets a product run an audio or video call without
implementing, embedding, or observing any realtime transport. The host owns the
connections, the capture devices, the codecs, the audio route, and the video on
screen. The product carries opaque signalling over a channel it already has,
says where each participant's video belongs, and is told how the call is going.

The first implementation is WebRTC, with the host running the peer connections.
The service names no transport, so what a product sees does not change if
another one is added.

## Motivation

A user cannot make a call from a product. No host service carries real-time
audio or video, so a messaging product can show a contact and exchange messages
with them, and still cannot start a voice or video call with them.

The only way to do it today is for the product to drive WebRTC itself. A web
product can: `RemotePermission::WebRtc` ungates the sandbox's own
`RTCPeerConnection`. The camera and microphone feed and every participant's
network address then pass through product code. Every other kind of product has
no route at all.

Capture devices, hardware codecs, the audio session, and the OS prompts are
host-owned, so a product cannot do this well. A media stack inside a product
defeats the device permissions that exist to keep frames out of it. And one
stack per product means one set of bugs per product, with no single place for
the user to see or stop a call.

## Approach

### Concepts

A **session** is one call. It has a host-minted `MediaSessionId`, belongs to the
product that created it, and holds one or more remote **participants**. A
participant is a host-minted handle, meaningless outside its session: enough to
say which peer a signalling message is for, whose tracks arrived, and whose
video a rectangle draws. The host is told no product-side identity, and the
product keeps its own mapping from handle to contact. Nothing in the transport
needs to know who anyone is.

A **signalling message** is an opaque, host-sealed byte string, addressed to one
participant. The host emits them, the product delivers them over its own
channel, and feeds received ones back. The product learns nothing from the
bytes: no session description, no candidate, no address.

A **media key** is a long-lived host key pair. The host hands a product the
public half, the product publishes it to its peers however it already
distributes keys, and a caller seals its invitation to it. So an invitation can
be stored and forwarded by a product that cannot open it; the host unseals it
when the call is answered. Later messages use a per-session key the host derives
during negotiation. Tampering or replay fails the session rather than disclosing
anything.

A **surface rectangle** places one participant's video, in the coordinates of
whatever surface the product already draws into. The product chooses the
rectangle, the corner radius, and the depth relative to its own content.

### Service

`create_session` says which local tracks to send, prompts the user, and returns
the session id. It connects nothing on its own.

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
- which local devices are actually live, which is not the same as what the
  product asked for: a withdrawn permission or another app taking the camera
  changes it unprompted.

`deliver_signalling` feeds a received message in. `set_local_tracks` mutes the
microphone and enables, disables, or flips the camera. `set_audio_route` picks
earpiece, speaker, or system, and loses to anything the OS routes itself.
`set_surfaces` replaces the whole rectangle set at once, so a layout change is
atomic. `end_session` hangs up on everyone, is idempotent, and is always
allowed.

### The host draws the video

The product sends rectangles and never receives frames. Every host already
composites the product's own surface — a canvas, a web view, a native view — so
a video layer is a sibling it positions from the rectangles the product gave it.
Nothing here depends on how the product renders.

The alternative, handing decoded frames to the product as textures, is rejected:
it puts camera output inside the product, and it copies every frame across the
product boundary for nothing.

The product therefore cannot read, filter, capture, or post-process call video,
and a rectangle is a request the host may clamp to what is actually visible.

### Consent and addresses

Capture uses the existing `Camera` and `Microphone` permissions; no new device
permission. Beyond that:

- Connecting exposes the user's address to the other participants, so a call
  needs an explicit decision before the first signalling message leaves the
  device. Starting a call, joining one, and answering an invitation all go
  through `create_session`, so all three take that decision; inviting a further
  peer into a call the user is already in does not ask again.
- Withdrawing camera or microphone access ends the session.
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
is inside the same boundary as the rest of the product: the invitation is sealed
to the media key, so the worker forwards bytes it cannot open, and no call hands
it an address, a candidate, or a device identifier. It learns only what it knew
already — which of its own contacts is calling.

Waking a product that is not running is out of scope here. It is a
media-neutral problem — every product that reacts to a remote event needs the
same thing — and will be dealt with separately. Until then, a call reaches a
product that is already open.

## Trade-offs

- Products cannot touch call media. That is the point, and it forecloses
  product-drawn effects and overlays on video.
- Opaque signalling means no interoperability with a third-party dialect.
- Screen sharing, recording, and product-chosen codecs are out; each needs its
  own consent story.
- A host must supply the whole stack — engine, capture, echo cancellation, audio
  session, connectivity, compositing — or report the service unsupported. There
  is no partial mode.
- Group calls work without a server, but not at arbitrary size. A host that
  wants large calls needs infrastructure this RFC does not describe.
- The media key is a long-lived host identity. Its rotation and revocation need
  specifying, and the same public key seen by two products tells them they are
  talking to one device.

## Open questions

- Does per-call consent reuse `RemotePermission::WebRtc`, or need its own
  variant? Reuse keeps the catalogue small but gives a browser-shaped remembered
  grant a second meaning.
- May a call start without a visible surface, so audio can connect while the
  product is still opening? That would let a background worker create an
  audio-only session.
- Is a coarse quality level worth sending at all, given a product cannot act on
  the reason behind it?
