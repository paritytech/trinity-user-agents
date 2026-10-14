---
title: "Wire message type: an explicit byte for trait, method, and leg"
owner: "@decrypto21"
---

# RFC 0027: Wire Message Type: An Explicit Byte for Trait, Method, and Leg

|                 |                                                                                                                  |
| --------------- | ---------------------------------------------------------------------------------------------------------------- |
| **RFC Number**  | 27                                                                                                                 |
| **Start Date**  | 2026-08-31                                                                                                         |
| **Description** | Replace the flat `(trait, method)` wire discriminant with `(trait, method, message_type)`, so method ids run as a dense per-trait sequence instead of a request/response pair or subscription quartet spread. |
| **Authors**     | Nidish                                                                                                             |

## Summary

The `(trait, method)` pair that addresses every TrUAPI frame is the same shape as a Substrate extrinsic's `(pallet_index, call_index)`: one byte names the module, the other names the operation within it. Codec 2's envelope (`[requestId][trait: u8][method: u8][payload]`) still spends that per-trait operation budget on which leg of a method's exchange a frame carries, not just the operation: a request/response method reserves two consecutive ids, a subscription reserves four. This RFC adds a third outer byte, `message_type`, that names the leg directly — `Request`/`Response` for request/response methods, `Start`/`Receive`/`Interrupt`/`Stop` for subscriptions — so the `(trait, method)` address stays exactly as flat as it is today (one `MethodIds` constant per method, unchanged as the dispatcher's routing key) while each method costs exactly one id regardless of how many legs its exchange has. `message_type` sits beside `trait` and `method` in the outer envelope, not nested inside the payload: a frame's leg is legible without decoding a single byte of its payload, and the payload itself is nothing more than that leg's own already-versioned wrapper type, SCALE-encoded exactly as it would be if it were the only shape that method ever had.

## Motivation

Codec 2 already fixed the *global* fragmentation problem: every trait now owns a contiguous 256-value method-id block instead of competing for one flat byte. But within a trait, the id budget is still spent two-at-a-time or four-at-a-time on something that is not really part of the method's identity: which leg of the exchange a given frame carries. `system_get_product_context` is method ids 8 and 9; a four-frame subscription like `account_connection_status_subscribe` is ids 0 through 3. That pattern means:

- **The 256-value ceiling arrives twice as fast for request/response methods, four times as fast for subscriptions.** A trait with 60 subscription methods exhausts its id space at 15 methods, not 60.
- **A method's shape is locked in by its ids, not by anything reversible.** If `foo_request` needs to grow an interrupt leg between request and response after it has already shipped, that is not additive under the current scheme: it is a new set of wire ids replacing the old pair, i.e. a breaking removal plus a breaking addition, and it pushes back every other method's id in the same trait that follows it. There is no way to express "this method grew a leg" without touching the wire table.
- **Public release is the last point this can move for free.** Once products in the field are decoding on any fixed shape of this envelope, every registered id becomes permanent; this restructuring is free today and a second breaking wire cutover after release.

This RFC folds those concerns into the outer envelope, at the same moment codec 2's own cutover (`WIRE_CODEC_VERSION` 1 → 2) is already in flight and unreleased.

An earlier draft of this RFC proposed pulling a method's *version* out to a wire byte alongside direction, wrapped in two new hand-maintained generics (`Request<Req, Res>`, `Subscription<Start, Item, Err>`) that every generated version enum would wrap its payload in. In practice, version is better left exactly where it already lives: inside each leg's own SCALE-encoded wrapper type, as that wrapper's own enum tag. A method's `Response` leg and its `Request` leg do not need to share a version number, let alone a subscription's four legs; forcing them under one umbrella tag is a coupling this RFC does not need to introduce to solve the id-budget problem. What actually wants to be visible one decode step before the payload is *which leg* a frame carries — routing and debug tooling need that to make sense of the bytes that follow, and it is the axis the id-budget problem is actually about. Version stays fully decoupled per leg, exactly as before.

## Detailed Design

### Current shape (codec 2)

```text
[requestId: SCALE str][trait: u8][method: u8][payload bytes]
```

`method` is not one id per method: `n` for a request, `n+1` for its response, or `n..n+3` for a subscription's start/stop/interrupt/receive. Routing happens on those bytes alone; which leg a frame carries is only knowable by which id it arrived on.

### Proposed shape

```text
[requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes]
```

One method id now serves every leg a method has. `message_type` is a plain `u8`, not a SCALE enum tag on some shared generic — the dispatcher already knows, from a method's own registration, which shape (request/response or subscription) it expects, so the two families reuse the same small integers rather than spreading across one combined space:

```text
MESSAGE_TYPE_REQUEST   = 0   MESSAGE_TYPE_START     = 0
MESSAGE_TYPE_RESPONSE  = 1   MESSAGE_TYPE_RECEIVE   = 1
                             MESSAGE_TYPE_INTERRUPT = 2
                             MESSAGE_TYPE_STOP      = 3
```

`Request` and `Start` share `0`, `Response` and `Receive` share `1`, in the same position, so a subscription's first two legs partly decode the same way a request/response method's frames do.

There is no generic wrapper type carrying a leg's payload. Each leg's payload bytes are exactly that leg's own already-versioned wrapper, encoded as it would be on its own:

- **Request**: `{Method}Request`'s own encoding (its `V1`/`V2`/... tag is the sole version signal for this leg).
- **Response**: `Result<{Method}Response, CallError<{Method}Error>>`, both sides already-versioned wrappers.
- **Start**: the request wrapper's own encoding, or zero bytes when the subscription takes no request at all.
- **Receive**: the item wrapper's own encoding.
- **Interrupt**: `Option<CallError<{Method}Error>>` — `None` is natural completion, `Some(err)` is a failure, replacing the previous silent-empty-frame convention with an explicit, decodable case. A subscription with no domain-specific error uses a bare `GenericError` in the same position.
- **Stop**: zero bytes, unconditionally — there is nothing left to version once a subscription is being torn down.

A method's version history is therefore not one shared sequence across all its legs; each leg versions independently, exactly as a lone request/response method's request and response already did before this RFC.

### Routing is unchanged

The dispatcher still keys on `(trait_id, method_id)`, one lookup. `message_type` is a second-level check the handler for that method already expects: a request/response registration accepts `Request` inbound and answers `Response`; a subscription registration accepts `Start`/`Stop` inbound and answers `Receive`/`Interrupt`. A frame carrying a `message_type` its registered handler does not expect (an outbound-shaped tag arriving inbound, or an unrecognized value) is a protocol violation. It is logged and dropped rather than answered: the pair is one this build implements, so a protocol error would misreport it as unsupported, and answering an inbound `Response` with a `Response` is how an error loop starts. An undecodable *payload* on an expected leg still answers with `CallError::MalformedFrame`.

The reserved `(255, 255)` address is the exception, because it is the one address every peer answers on. A protocol error whose version or variant index this build does not recognize settles the correlated call and leaves the frame and the connection intact, rather than being rejected: a peer that refused an unfamiliar payload here could never be told anything new without the connection dying, which would freeze that channel at whatever shape shipped first. A recognized shape stays strict, since a malformed one is corruption rather than a newer peer.

### Adding a leg later

Because `message_type` is not derived from a method's ids, growing a method's shape — the interrupt-after-a-few-months case that motivated this RFC — is purely additive: a new `message_type` value and a new encode/decode arm in that method's dispatch entry, with no change to `(trait, method)` and no pressure on any other method's id in the same trait.

### Compatibility

This folds into codec 2: `WIRE_CODEC_VERSION` stays `2`. The codec-1/codec-2 boundary is entirely about the first (trait) byte, and codec 1 is no longer decodable at all, so no id-range constants survive to guard it. Codec 2 has not shipped yet, so this is a zero-cost renumbering: there is no codec-2 peer anywhere to break a second time. Folding it into the same unreleased cutover avoids a third wire-breaking version bump before public release.

## Drawbacks

- One more byte on every frame (the `message_type` byte; nothing was removed to pay for it, since the old scheme's "direction" was implicit in which id a frame used rather than a byte on the wire).
- Codegen must special-case each method's leg set explicitly — a per-method match over `message_type` — rather than decoding through one shared generic. This is more codegen surface per method than a single `Request<Req, Res>`/`Subscription<Start, Item, Err>` wrapper would need, but it removes the generic entirely from the generated client's call sites: a generated method calls a leg's own wrapper codec directly rather than constructing a `Request::Request(...)`/`Subscription::Start(...)` variant first.
- A subscription's `Interrupt` leg needs its own error type parameter per method (or the shared `GenericError` fallback) at the codegen layer, rather than inheriting one fixed `Err` slot from a single generic — a small amount of additional bookkeeping in the generator, not in generated or hand-written call sites.

## Testing, Security, and Privacy

Regenerating the golden and fixture tests listed above is the primary verification surface: byte-level goldens (`golden-account-get.bin`, the wire-table/dispatcher parity tests) must be recomputed, not relaxed, exactly as codec 2's own cutover recomputed them rather than loosening their assertions. No new security surface is introduced; `CallError::MalformedFrame` is the correct response to a `Response`/`Receive`/`Interrupt`/`Stop` tag arriving where a `Request`/`Start` tag is expected, rather than accepting it silently.

## Performance, Ergonomics, and Compatibility

### Performance

Routing remains a single `HashMap<(u8, u8), _>` lookup, unchanged in complexity. The added cost is exactly one extra single-byte read per frame for `message_type`, negligible next to decoding the payload itself, and strictly cheaper than the previous scheme's implicit direction-from-id lookup plus a version tag one decode step into the payload.

### Ergonomics

Method ids become a dense, gap-free per-trait sequence (0, 1, 2, ...) instead of today's 0, 2, 4, 6, 8 pattern, which is easier to read off a wire dump and easier to eyeball for accidental collisions in review. `message_type` is legible directly off the outer envelope, before any payload decode, which is what makes debug tooling (`truapi-debugger`) able to resolve a frame's role from the wire alone rather than peeking into payload bytes. A method that starts as plain request/response and later needs a streaming or control leg expresses that as a new `message_type` value and dispatch arm, rather than as a set of wire ids replacing another set.

### Compatibility

Covered above: folds into codec 2, no additional version bump.

## Future Directions and Related Material

Nothing here reserves a range of trait ids. The only address this scheme holds back is `(255, 255)`, for method-independent protocol errors; a later RFC could revisit that if the protocol-error channel ever needs more than one address.
