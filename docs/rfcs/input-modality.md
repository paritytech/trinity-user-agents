---
title: "Input Modality"
owner: "@johnthecat"
status: draft
---

# RFC — Input Modality

|                 |                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-07-28                                                                                        |
| **Description** | Route user input to the products already on screen, as a contextual surface over another modality |
| **Authors**     | Sergey Zhuravlev                                                                                  |

## Summary

Input is a modality with no screen of its own. The host opens it as a surface over the view the user is on, and **the
screen underneath dictates who is guaranteed to receive the input**. Those products are always asked, provided they
accept the input's shape, and their answers rank first. A host may ask others at its own discretion; their answers rank
below. Each product answers with candidates or declines. A candidate is text, an attachment, or a widget the product
renders itself. Widgets are drawn through the `Renderer` trait of [Unified Renderer](unified-renderer.md). A navigation
is the one exception: it names a product and a surface, and replaces the context instead of acting within it.

## Motivation

A view offers the interactions its product designed into it. A user may want something the view did not anticipate.

Two things are missing:

- **A product cannot be handed anything.** The only way to reach one is to open it, so every input is an entry point.
  There is no channel to a product the user is already looking at.
- **A product cannot answer.** A product holds state the host cannot see, so a string may mean something to it and to
  nothing else. The set of such strings is open, so the host cannot close the gap by learning formats.

Making input contextual closes both: the screen names its products, and those are the products asked.

### Requirements

1. **Contextual.** Input reaches the products the user is looking at, without leaving the view. Products the user cannot
   see are not the default recipients.
2. **Products interpret, the core does not.** The host carries input uninterpreted. Which strings mean something is a
   product's knowledge, and a new meaning must not require a protocol or host change.
3. **Answers, not just delivery.** A product can respond to input, and the host presents responses from several products
   in one place, each under its own identity.
4. **Bounded disclosure.** Input goes to no product without either the screen or the host's stated policy putting it
   there.
5. **Confirmed external input.** Input authored outside the host is routed only after the user confirms it.

## Explanation

The user opens the input surface over the screen they are on. That screen may be an app view, a chat view, or a pocket
view. The host reads it for its **context set**: the products present in it that accept the input's shape. It asks all
of them at once, ranks them first, and each answers with candidates or declines. Beyond them the host may ask further
products at its own discretion, ranked below. A navigation names a product and a place inside it outright, and moves
there instead.

The design has five parts:

- **Registration**: what a product publishes to take part.
- **Data types**: what an input is.
- **Context**: who is guaranteed to receive it.
- **Query answering**: what comes back.
- **Worker lifecycle**: when workers run.

### Registration

A product takes part only if its worker manifest says so. The [Product Manifest Format](product-manifest.md) defines the
worker executable at `worker.<product_id>.<tld>` and its `includes` key, which names `input` beside `chat` and `pocket`.
`includes.input` is an object listing the shapes the worker accepts; the Product Manifest Format is amended to match. A
worker without it is never asked anything.

```json
{
  "kind": "worker",
  "entrypoint": "./worker.js",
  "includes": {
    "chat": true,
    "pocket": false,
    "input": {
      "supports": ["text", "image", "audio", "video", "file"]
    }
  }
}
```

**`supports`** lists the input shapes the worker accepts: `text`, `audio`, `video`, `image`, and `file`. A text query
goes only to workers listing `text`; an attachment only to workers listing its category. An unrecognized member is
ignored. An empty array means the worker is never asked.

### Data types

#### The routed input

A routed input says one of two things: **open this surface of this product**, or **here is a query for the products in
context**. Only a query reaches a product.

```rust
/// What the host resolved a user input to.
pub enum RoutedInput {
    /// Open the product's app view at a path within it.
    App { product_id: String, pathname: String },
    /// Open one room of the product's chat view.
    Chat { product_id: String, room_id: String },
    /// Open one artifact in the product's pocket view.
    Pocket { product_id: String, artifact_id: String },
    /// Something to be answered, uninterpreted by the host. Text the host
    /// resolved to no product falls through to here.
    Query(Query),
}

/// An input no product was named for.
pub enum Query {
    /// The user's text, exactly as they entered it.
    Text(String),
    /// Something the user handed to the host, carried whole.
    Attachment(Attachment),
}

/// A user-supplied payload, categorized for handling.
pub enum Attachment {
    /// Audio the host can play inline.
    Audio(AttachmentData),
    /// Video the host can play inline.
    Video(AttachmentData),
    /// An image the host can render inline.
    Image(AttachmentData),
    /// Anything else, such as a document or an archive.
    File(AttachmentData),
}

/// The bytes of an attachment and what they are.
pub struct AttachmentData {
    /// Name as the user knows it.
    pub file_name: String,
    /// MIME type.
    pub mime_type: String,
    /// The bytes themselves.
    pub content: Vec<u8>,
}
```

Every input goes through three steps in the host before any product hears of it:

1. **Capture.** The host takes the input from its own surface: a typed string, a scanned code, or a payload handed over
   by the operating system.
2. **Classify.** If the input is a deeplink to a modality, it becomes `App`, `Chat`, or `Pocket` according to its
   content. Anything else becomes `Query`: a string is `Query::Text`, a payload is `Query::Attachment`.
3. **Navigate or ask.** A navigation is executed by the host: it opens the named surface and delivers nothing to any
   product. A `Query` is put to the context set of the screen it was entered on, as one round.

`Query` is the only variant that crosses the wire. `Text` crosses unmodified: every product receives exactly what the
user entered, and interpreting it is the product's job. The `Attachment` category is derived, not declared: the host
maps `image/*`, `audio/*`, and `video/*` to their variants and everything else to `File`. `mime_type` is never empty.
When the attachment arrives without one, the host may derive it; if that fails, it falls back to
`application/octet-stream`.

#### When routing happens

Input the user composed inside the host is routed when the host decides: on a pause, on submit, or otherwise. Rounds
supersede, so the user sees the answer to the string currently in the field.

Input authored outside the host is routed only after the user confirms it. For a scanned code the host shows what it
decoded and what it will do with it. For input from the operating system a share sheet or file picker already showed the
user the payload and counts as confirmation; a bare tap on a link does not, and the host confirms it again.

### Context

The context set is the products on the screen the input surface opened over whose `supports` lists the input's shape.
Every product in it is queried, and its answers rank first. The host may also ask products outside the set, if they
declare `includes.input`; which ones, and whether any, is host policy. Their answers rank below every context candidate.
Ranking within each band is host policy.

Context set examples by modality underneath:

| Modality | Context set                                        |
| -------- | -------------------------------------------------- |
| App      | The one product whose app view is on screen.       |
| Chat     | Every product with a room registered.              |
| Pocket   | Every product with an artifact in the pocket view. |

### Query answering

The host calls the worker with the query, and the return value is the answer.

```rust
/// One query the host routed to this product.
pub struct ProductInputRequest {
    /// What was routed.
    pub query: Query,
}

/// What a product has to say about one query.
pub enum InputResponse {
    /// The product has nothing to offer. Terminal: the stream ends after it.
    NotHandled,
    /// Zero or more answers, ordered by the product's own confidence.
    Candidates(Vec<InputCandidateContent>),
}
```

#### Candidate content

```rust
/// Body of a candidate. `Text` and `Attachment` mirror `Query`.
pub enum InputCandidateContent {
    /// Plain text.
    Text(String),
    /// A payload the candidate offers, carried whole.
    Attachment(Attachment),
    /// A candidate the product draws itself.
    Custom(InputCustomContent),
}

/// A candidate whose body the product renders and whose controls it handles.
pub struct InputCustomContent {
    /// Identifies this candidate among the ones this product answered with.
    /// Correlates the render call and any action triggered in it.
    pub candidate_id: String,
    /// Product-defined discriminator used to select a renderer.
    pub content_type: String,
    /// Product-defined payload, opaque to the host.
    pub payload: Vec<u8>,
}
```

Every candidate renders under the answering product's `displayName` and `icon` from its root manifest. `Text` and
`Attachment` are drawn by the host.

A `Custom` candidate is drawn by the product through the `Renderer` trait of [Unified Renderer](unified-renderer.md).
Input adds one variant to its `RenderContext`:

```rust
/// A candidate answered to an input query.
InputWidget { candidate_id: String, content_type: String },
```

The host hands `payload` back under that context, with the candidate's `content_type` in it, and asks for a renderer
tree, which it draws inside its own frame. Controls in the tree deliver actions to the product.

#### The `Input` trait

```rust
/// Contextual input routed to the product's worker.
pub trait Input: Send + Sync {
    /// Route one query to this product's worker and take its answer.
    fn request(
        &self,
        _cx: &CallContext,
        _request: ProductInputRequest,
    ) -> Subscription<InputResponse, Result<(), CallError<GenericError>>> {
        Subscription::empty()
    }
}
```

`Input::request` is `host_initiated`, the existing primitive for the host calling a product and taking a stream. The
product streams `InputResponse` items and the host appends each one's candidates to that product's list. The stream ends
with an interrupt carrying `Result<(), CallError<GenericError>>`
([Subscription Typed Interrupt Payload](subscription-typed-interrupt-payload.md)): `Ok` means the product is done, `Err`
means it failed. Either way the host keeps the candidates received. `NotHandled` is terminal too; the host closes the
stream after it. The host ends the call itself on supersession or the response deadline.

### Worker Lifecycle

Workers are reference-counted; the rule is [Worker Lifecycle](worker-lifecycle.md). Input adds two references:

- An **open input surface**, on every product it queries, for as long as the round is open.
- A **displayed `Custom` candidate**, on the product drawing it, for as long as its render call is open.

## Drawbacks

- The same string typed over two screens gets two answer sets, not cachable.
- How far beyond the screen a query travels is host policy, so privacy differs between hosts.
- A large pocket or room queries every product in it, with no upper limit.
- A host that routes while the user types discloses prefixes.

## Performance, Ergonomics, and Compatibility

A query costs one concurrent call per product asked, bounded by the response deadline, and at most one round is in
flight. A `Custom` candidate costs a second call that stays open while it is drawn.

A product participates by setting `includes.input` and implementing `Input::request`. `Text` candidates are enough for a
working answer; `Renderer` is needed only to draw custom ones. What a product answers needs no manifest change.

`Input` is additive on the wire: a product that does not implement it receives no queries. The manifest change is
breaking: `includes.input` becomes an object, a boolean value is malformed, and the Product Manifest Format is amended
to match. A worker manifest without `includes.input` stays valid.

## Prior Art and References

- [Product Manifest Format](product-manifest.md)
- [Worker Lifecycle](worker-lifecycle.md)
- [Subscription Typed Interrupt Payload](subscription-typed-interrupt-payload.md)
- [Unified Renderer](unified-renderer.md)

## Unresolved Questions

1. Widget context: which products a dashboard contributes. Until settled, the input surface does not open over one.
2. Whether a product is told if it is alone on screen or one of several.
3. Input syntax: the textual form that produces `App`, `Chat`, and `Pocket`. This blocks anything printed or shared
   between hosts.
4. Ranking within a band, given that any product can return a plausible candidate for every query.
5. Whether a host that asks beyond the screen must let the user exclude a product.
6. Whether ranking outside products by selection history or answer rate is acceptable profiling.
