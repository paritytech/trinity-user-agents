---
title: "TrUAPI Protocol Design"
type: design
status: accepted
created: 2026-03-13
---

# TrUAPI Protocol Design

## Overview

The TrUAPI protocol connects a **Product** — a web application — with its **Host**, the native Polkadot application that embeds it. The two run in separate execution contexts (an iframe or webview inside a native shell) and share no memory; everything they exchange has to cross a process boundary as raw bytes.

This document specifies the **transport layer**: the rules for turning a method call on one side into bytes on the wire and back into a typed result on the other. It deliberately stops there. The concrete call surface — the methods themselves, their request/response types, error enums, and the wire-protocol discriminant ids — is defined in the `truapi` crate (`rust/crates/truapi`) and the clients generated from it. Keeping the two apart lets the API surface grow without disturbing the transport rules underneath it.

TrUAPI is language-agnostic. The protocol assumes nothing Rust-specific; any language that can serialize the same byte layout can speak it.

## Technical Requirements

A Host and a Product may be built on different platforms — web, iOS, Android — and in different languages. The transport therefore makes no assumptions about either side beyond a shared byte channel:

- The protocol MUST provide a transport layer between Host and Product over an arbitrary byte channel.
- The message format MUST be well-defined and serializable, so that an encoder on one platform and a decoder on another agree byte-for-byte.

## Transport

Communication between Host and Product can be carried over any IPC mechanism — a `MessagePort`, `postMessage` across an iframe boundary, or anything else that moves bytes. The transport treats that channel as opaque: the body of each IPC message is a single serialized `Message` (a byte array), and how those bytes actually travel is left to the environment.

Because the channel carries nothing but bytes, both sides must agree precisely on how a `Message` is laid out. That agreement is the serialization format.

### Serialization

Messages are plain structs and enums that are serialized into bytes on one side and decoded back into the same shape on the other.

Message serialization is built on [SCALE codec](https://github.com/paritytech/parity-scale-codec). The codec is positional — it writes no field names or tags, only values in declaration order — so **the field order of structs and the variant order of enums are part of the wire contract**; reordering them silently breaks compatibility. The examples in this document omit the codec derive calls, but they are always implied. `Result` is treated as an ordinary serializable enum.

#### Note on `Compact`

SCALE encodes integers in fixed width by default. The `Compact` type provides a variable-length encoding that keeps small numbers small on the wire — a single byte for the common small values.

### Interface

Every message on the wire shares one envelope:

```rust
struct Message {
  requestId: str,
  payload: Payload
}
```

`requestId` ties related messages together (see [Rules](#rules)); `payload` carries the action itself. On the wire the envelope is laid out as:

```text
[requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes...]
```

The three bytes after the `requestId` are the **`(trait, method, message_type)` triple**. The first byte identifies the API trait (`System`, `Account`, `Chain`, ...); the second identifies a method within it: exactly one id per method, regardless of that method's shape; the third, `message_type`, names which leg of that method's exchange this frame carries (see below). The payload bytes are the SCALE-encoded value for that leg's own already-versioned wrapper type, inlined without a length prefix; the receiver reads to the end of the transport frame.

Trait discriminants are assigned per trait in the `truapi` crate via the trait-level `#[wire_trait(id = N)]` annotation, with the `System` trait fixed at `1`, so a handshake request frame always starts `[requestId][0x01][0x00]`. Each method carries an explicit discriminant within its trait, assigned via the `#[wire(id = N)]` annotation and numbered from `0` independently inside every trait. Ids are **append-only per trait and never reused**: once a `(trait, method)` pair ships it keeps its meaning forever, which is what lets a newer Host and an older Product still understand each other, and adding methods to one trait never disturbs the ids of any other trait. The crate is the source of truth for all values. Trait discriminant `255` is permanently reserved for protocol errors and cannot be assigned to an API trait, so no method can ever be addressed there; a protocol error travels on the pair `(255, 255)`.

#### The message type byte

A `(trait, method)` pair names a method, not a leg: request and response share it, and so do a subscription's four phases. Which leg a frame carries is `message_type`, a third byte in the outer envelope, not something nested inside the payload:

```text
MESSAGE_TYPE_REQUEST   = 0   MESSAGE_TYPE_START     = 0
MESSAGE_TYPE_RESPONSE  = 1   MESSAGE_TYPE_RECEIVE   = 1
                             MESSAGE_TYPE_INTERRUPT = 2
                             MESSAGE_TYPE_STOP      = 3
```

`Request` and `Start` share `0`, `Response` and `Receive` share `1`, in the same position: a subscription's first two legs occupy the same slots a plain request/response method's two legs would, so the byte alone plus the method's registered kind (never both a request/response method and a subscription) resolves unambiguously.

The payload bytes that follow `message_type` are exactly that leg's own already-versioned wrapper type, SCALE-encoded as it would be if it were the only shape that method ever had — there is no further nesting and no separate version byte:

- **Request**: `{Method}Request`'s own encoding; its `V1`/`V2`/... tag is the sole version signal for this leg.
- **Response**: `Result<{Method}Response, CallError<{Method}Error>>`, both sides already-versioned wrappers.
- **Start**: the request wrapper's own encoding. A subscription that carries no request data still sends one, whose `V1` holds no payload.
- **Receive**: the item wrapper's own encoding.
- **Interrupt**: `Result<(), CallError<{Method}Error>>`, from the provider — `Ok(())` is natural completion, `Err(err)` is the value the subscription ended with. A subscription with no domain-specific error uses a wrapper whose `V1` is `GenericError`, so the interrupt versions like every other leg.
- **Stop**: zero bytes, unconditionally, from the consumer. Either side can be the one to end a stream, which is why both legs exist.

Each leg therefore versions independently: a method's `Response` does not share a version number with its `Request`, nor do a subscription's four legs share one with each other.

A method's shape is fixed for the life of its `(trait, method)` pair. Nothing in the envelope selects between shapes, and dispatch registers a pair as either a request or a subscription, accepting only that shape's inbound legs. A frame carrying a leg its registered shape cannot receive is logged and dropped, not answered: the pair is one this build implements, so a protocol error would misreport it as unsupported, and answering an inbound `Response` with a `Response` is how an error loop starts. A method that has to become the other shape takes a new method id; the old id keeps answering the old shape for as long as any peer still speaks it.

For example, a `system_feature_supported` request/response pair (trait `1`, method `1`) is carried as:

```text
outbound (Request):  [0x01][0x01][0x00 REQUEST][0x00 V1][...request fields]
inbound  (Response): [0x01][0x01][0x01 RESPONSE][0x00 Ok][0x00 V1][...response fields]
```

and a subscription's four legs all address the same `(trait, method)` pair, distinguished only by `message_type`:

```text
start:     [trait][method][0x00 START][0x00 V1][...start fields]
receive:   [trait][method][0x01 RECEIVE][0x00 V1][...item fields]
interrupt: [trait][method][0x02 INTERRUPT][...Result<(), CallError<Err>> bytes]
stop:      [trait][method][0x03 STOP]
```

Request/response and subscription methods are both derived mechanically from the TrUAPI trait methods, so the high-level method signature and the wire format can never drift apart; nothing is written by hand.

### Rules

A single byte channel carries every call in both directions at once, so the two sides need a way to tell which message belongs to which exchange. That is what `requestId` is for.

#### Requests

Every request expects exactly one response. Each Host or Product MUST send a response message for every request it receives, and the request and its response MUST share the same `requestId` — so the caller can match a reply to the call it made even with many calls in flight.

If a receiver has no handler for an incoming `(trait, method)` pair, it MUST send a protocol-error frame addressed to `(255, 255)` with the same `requestId`, `message_type` set to `MESSAGE_TYPE_RESPONSE`, and payload `V1(UnsupportedMessage { trait_id, method_id })` — encoded as the four bytes `[0, 0, unsupported_trait, unsupported_method]` — one byte cannot name a pair, so the error that describes the envelope grew with it. The sender maps this method-independent response to its own pending request or subscription and reports a generic unsupported error. A receiver MUST NOT answer a protocol-error frame with another protocol error.

A protocol-error frame MUST NOT receive another protocol-error response. An unmatched error is ignored. A protocol-error payload whose variant index the receiver does not recognize MUST settle the correlated call and leave the frame and the connection intact: `(255, 255)` is the one address every peer answers on, so a receiver that rejected an unfamiliar payload here could never be told anything new without the connection dying, which would freeze this channel at whatever shape shipped first. A payload whose variant IS recognized stays strict, and a malformed one is rejected as a wire violation. These rules prevent error loops and keep the channel extensible without hiding corrupt control messages.

Hosts and Products released before this control frame was introduced still silently drop unknown discriminants. They must be upgraded once before they can safely reject APIs introduced by later peers. Codec version 1 frames are not decodable under this envelope at all: the handshake itself rides the changed header, so a codec-1 peer cannot be negotiated with in band.

#### Subscription

A subscription is not a one-shot call but an ongoing stream: the consumer asks once and then receives updates until it stops listening. Its four messages (`start`, `stop`, `interrupt`, and `receive`) all address the same `(trait, method)` pair (distinguished by the `message_type` byte in the outer envelope) and MUST all share the same `requestId`, so a subscription handler can route every update and teardown signal to the right place.

Each message has a defined role:

- `start` — the consumer subscribes; it MUST send a `start` message to the provider.
- `stop` — the consumer unsubscribes; it MUST send a `stop` message.
- `interrupt` — the provider MUST end a stream with an `interrupt` message, carrying `Ok(())` when it has no more data to supply and `Err(reason)` when it stopped for a reason the consumer can act on.
- `receive` — the provider MUST deliver each update with a `receive` message.

When a protocol error rejects a subscription's `start` frame, the consumer
MUST terminate that subscription with a generic unsupported error and MUST NOT
send a `stop` frame in response.

The returned `Subscriber` interface depends on the implementation, but a generic one may look like this:

```rust
struct Subscriber {
  unsubscribe: fn(),
  onInterrupt: fn(fn())
}
```

### Handshake

Before either side trusts a single byte of payload, they have to agree on how those bytes are encoded. That negotiation is the handshake, and it runs first.

Handshake calls are bidirectional: both Host and Product can send a handshake request, and both MUST respond to one. An implementation CAN apply a timeout of 10 seconds, after which the connection is marked failed and the call returns a timeout error. The handshake result can be cached.

The handshake request carries the protocol (codec) version as a `u8`. On receiving it, the peer switches its encoding/decoding mode to match; for the SCALE codec with the `(trait, method)`-addressed envelope, the version is `2`. (Codec version `1` designates the retired single-byte-discriminant envelope; a peer speaking it fails the handshake.) A successful handshake MUST be the first request TrUAPI processes — any other request sent before a successful handshake response MUST fail.

The concrete handshake request, response, and error types are defined in the `truapi` crate.

