---
title: "Pocket modality"
owner: "Valentin Fernandez"
status: draft
---

# RFC — Pocket modality

|                 |                                                                                                 |
| --------------- | ----------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-09-03                                                                                      |
| **Description** | A host-owned collection of product-backed cards: how a card is added, rendered, opened, removed |
| **Authors**     | Valentin Fernandez                                                                              |

## Summary

Pocket is a host surface holding a small set of **cards**, each backed by a product. The host owns the collection and
renders every collapsed card natively from a renderer tree the product's worker streams to it. Tapping a card opens the
product's Widget executable, and a card that declares a back can be turned over there. A product cannot add a card by
itself: a card enters Pocket when the user follows a Pocket-targeted deeplink and approves a host dialog showing the
card as it will look. A link can also name an instance, which the product's worker labels for the dialog, so one card
definition can back many cards. The user removes a card, directly or through the owning product. Three privileged cards,
Humanity, Balance and Scarcity, are always present and removable by no one.

A plain card's face is drawn through the
[Unified Renderer](https://github.com/paritytech/trinity-user-agents/pull/633)'s `PocketCard` context and its back on
`PocketCardBack`; an instance's face and back are drawn on `PocketCardInstance` and `PocketCardInstanceBack`. The
collection itself is one `Pocket` trait with three methods, a Pocket section in the Worker manifest, and a host-owned
deeplink that names the Pocket. An opened card adds one `ExpandedCard` trait, through which the Widget moves the face
out of the way and back, and turns a card with a back over.

Tracking issue: [#563](https://github.com/paritytech/trinity-user-agents/issues/563).

## Motivation

The iOS host shows Humanity, Balance and Scarcity as cards whose content and interactions are hard-coded into it.
Personhood is becoming a product ([RFC 0024](0024-personhood-as-product.md)), which declares
`includes: { pocket: true }` and expects a card. No contract stands behind that flag: nothing says which cards a product
may back, how one enters the collection, or who can take it out.

The first iteration ships three host-placed cards before anyone publishes one, so the collection rules hold for a
host-placed card on their own and the add flow layers on top without changing them.

## Approach

### Model

- A **card** is identified by `(product_id, card_id, instance_id)`. `card_id` is a short id the product declares,
  screened by the same rules as a chat identifier (see Wire surface). `instance_id` is absent on a **plain card** and
  present on an **instance**, one of the cards a definition backs for an id a Pocket link names (see Instances). A
  privileged card is always a plain card. An instance's **label** is the short text the product's worker gives it,
  naming it as the user knows it.
- The **face** is the collapsed presentation, rendered natively from a `RendererNode` tree.
- The **back** is an optional second face a card's definition declares, drawn the same way and shown only by turning the
  opened card over. The face shown otherwise is the card's front. The fixed theme, `faceShown` and
  `ExpandedCard::set_face_shown` apply to the card whichever side is up. A turn is not a move of the face, and hiding
  and showing the face keeps the side that was up.
- The **expanded card** is the product's `widget` executable in a WebView.
- The **worker** is the product's one Worker executable. It draws the faces, handles their actions, and is the only
  execution the `Pocket` trait is available to.
- A **privileged card** is one the host itself places and pins: present on first run without approval, never removable.
  Iteration 1 ships exactly three, Humanity, Balance and Scarcity, and the host designates the product that backs each
  (the personhood provider of RFC 0024 for Humanity).

The host is the only writer of the collection. A product observes its own cards, labels an instance the user is about to
add, removes a card when the user asks it to, and does nothing else.

### Rendering and actions

A face is a body drawn through [Renderer](https://github.com/paritytech/trinity-user-agents/pull/633), on the
`PocketCard { card_id }` context. The host opens a `render` stream while the face is on screen and gets a `RendererNode`
tree per item; presses and edits inside the tree arrive on `action_subscribe` under the same context, so one handler
serves every card of the product. The back is drawn on `PocketCardBack { card_id }` and its presses arrive under that
context, so a product tells the sides apart by context alone. The host requests a back only for a card whose definition
declares one. An instance's face and back are drawn on `PocketCardInstance { card_id, instance_id }` and
`PocketCardInstanceBack { card_id, instance_id }` instead, so a product tells its instances apart by context too.

Pocket adds two rules on top. First, the host caches the newest tree per card and side durably, so a face is shown
offline and at cold start before the worker answers, and a privileged card has something to show on first run. Until the
worker draws the back, the host shows the cached back, else the archive file the definition names, and a back render the
worker refuses leaves that back in place rather than blanking it. A definition that stops declaring `back` discards the
cached back of every card it backs, and an opened card showing its back returns to the front.

Second, the host draws every face in one fixed theme, whatever theme the user picked, so a card looks the same in every
theme. In the Polkadot Android and iOS apps that theme is Berlin Night: `FgPrimary` is `#F4F4F5` and `BgSurfaceMain` is
`#0B0C0F`. `theme.subscribe` still reports the user's theme, for the product's own screens.

Presses inside a cached or archive back go nowhere, by the renderer's rule that only the current tree of an open
`render` stream is a source of actions. This also protects a product on an SDK without `PocketCardBack`: it refuses the
back render, so it never receives a back press it cannot decode, which would end its whole action stream. Its back shows
but does not respond, so a product declares `back` only once its SDK has `PocketCardBack`.

### Expanded card

Tapping a face opens the product's `widget` executable with the card named in the launch URL query, `card=<card_id>`,
followed by `instance=<instance_id>` for an instance, each percent-encoded outside RFC 3986's unreserved characters. The
Widget runs under the same product identity and storage namespace as the worker, so the card the user opened and the
state it shows are one product's.

The host draws the face above the Widget, and the user can move it out of the way and back. The Widget can do the same
with `ExpandedCard::set_face_shown`, to take the whole screen or to bring the face back after something on it changed. A
card definition's `faceShown` sets where the face starts when the card opens. The host sizes the Widget to the part of
the screen the face leaves, so the Widget learns the space it has from its own `resize` event. The face is never locked:
the user can always move it, and a call that arrives while the user is moving it or turning the card has no effect and
is answered `UserMoving`, so the product can retry once the user lets go. A product should not undo a move the user
made. Keeping the native face visible through the open and close animation, and preloading the WebView, are host
implementation and not part of the contract.

A card with a back is turned over on the opened card by dragging sideways or flicking, and the host offers the same turn
as an accessibility action for users who cannot drag. The turn follows the finger, always settles flat on one side, and
never shows a side mirrored. A card without a back does not turn, and a card in the collection never turns. Closing the
card returns it to its front. The host loads the back no later than when the card opens, so turning never waits on it.
On an opened card both sides count as on screen, an exception to the renderer's Live rule, since the back is one turn
away.

The Widget can turn the card too, with `ExpandedCard::set_back_shown`, for example from a "Show my code" button on its
page for a user who does not know the card turns. The call names the side to show rather than turning by half a turn, so
repeating it changes nothing, and it answers once the turn starts. A call while the user is dragging the card, to turn
it or to move the face, has no effect and is answered `UserMoving`. Asking for the back of a card with no back to show
right now answers `NoBack`, which may change once the worker draws one. Asking for the front of a card with no back
succeeds, since the front is up. A product should not undo a turn the user made. Turning while the face is moved away
changes the side the face shows when it comes back.

### Lifecycle

Pocket holds a worker reference for each side on screen, and one while it waits for the worker to label an instance,
which ends when the worker answers or the host stops waiting. A product cannot add a reference, only drop a card's
references by removing the card.

### Adding a card (full iteration)

Products **publish** card definitions in the Worker manifest ([Product Manifest Format](product-manifest.md)), alongside
the existing `includes`:

```ts
type WorkerManifest = CommonExecutableFields & {
  kind: "worker";
  entrypoint: string;
  includes: { pocket?: boolean; chat?: boolean; input?: boolean };
  /** Cards the product can back. Absent unless `includes.pocket` is true. */
  pocket?: { cards: PocketCardDefinition[] };
};

type PocketCardDefinition = {
  id: string; // Card id, unique within the product. Screened as a chat identifier is.
  title: string; // Shown in the approval dialog and in host chrome.
  preview: string; // Path inside the worker archive to a RendererNode tree, JSON in the generated TypeScript shape.
  faceShown?: boolean; // Whether the face is shown above the expanded card when it opens. Default true.
  back?: string; // Path inside the worker archive to the back's RendererNode tree. Absent: the card has no back.
};
```

A `faceShown` that is not a boolean, or a `back` that is not a string or is empty, makes the definition invalid, as a
missing `title` does, and an invalid definition drops all of the product's cards. A `back` file that is missing or does
not decode counts as no file: the card shows its cached back, or has no back until the worker draws one. A host that
does not read `faceShown` or `back` shows nothing wrong, so a product checks both before publishing rather than relying
on what today's hosts display.

The preview is a static file in the CID-pinned archive, so the host can show a plain card before any product code runs,
the same property the [funding modality](https://github.com/paritytech/trinity-user-agents/pull/339) relies on for its
rail list. It is the face the user approves; the live face may differ once the worker streams.

**Deeplinks name the Pocket.** A Pocket link is one of the app's own routes, beside `pair` and `pay`, with the product
as a parameter. Its host, `pocket`, has no TLD, so no dotNS product can take it, and a product URL
`polkadot://<product_id>.<tld>/<path>` always opens the App, whatever its path:

```text
polkadotapp://pocket                                                                    Open the Pocket
polkadotapp://pocket/add?product=<product_id>.<tld>&card=<id>[&instance=<instance_id>]  Offer to add a published card
polkadotapp://pocket/open?product=<product_id>.<tld>&card=<id>[&instance=<instance_id>] Expand a card that is present
```

`polkadotapp://pocket` with no action opens the Pocket itself, for a product that sends the user to the collection
without naming a card. Arguments are ignored. Expanding a card opens the Pocket on that card, the same view a tap on it
gives (see [Expanded card](#expanded-card)).

An `open` link's query parameters other than `product`, `card` and `instance`, URL-encoded and up to 2048 bytes, are
passed on to the card's page after `card` and `instance` when the link expands a card that is present, so a product can
pick the screen its card opens on. Anyone can write the link, so the page reads them as a hint, never proof. The same
holds for `card` and `instance` in the page's URL: a host that predates instances passes a link's `instance` on as a
page parameter.

The same link works from a product, a QR code or a web page. A link that names an action the host does not serve, or
names no product or no card, is refused with a message. Products reach a Pocket link from their own web UI through
`system.navigate_to`, which lets it through without the `OpenUrl` grant other `polkadotapp://` links need, so an "Add to
Pocket" button is one call. The `product` need not be the caller: the dialog shows the card, with an instance's label
from the product's worker, and approving it is the user's consent.

```text
User                Host                             Product worker
 |                   |                                     |
 |  polkadotapp://pocket/add?product=game.dot&card=loyalty
 |------------------>|
 |                   | resolve worker manifest, find card
 |                   | `loyalty`, fetch its preview from the archive
 |                   |
 |  dialog: title + rendered preview + Add
 |<------------------|
 |  Add
 |------------------>|
 |                   | insert card
 |                   |
 |                   |  start, register renderer handlers
 |                   |------------------------------------>|
 |                   |
 |                   | face on screen, reference 0 -> 1
 |                   |
 |                   |  renderer.render
 |                   |    { context: PocketCard { card_id: "loyalty" } }
 |                   |------------------------------------>|
 |                   |
 |                   |  face tree, and again on every change
 |                   |<------------------------------------|
```

An added card is an ordinary card from then on: same face stream, actions, expansion, removal rules, and worker
reference as a privileged one, without the pin. If the card is already present, `add` behaves as `open`, and if it is
not, `open` behaves as `add`, so a link to a card the user has yet to add leads to it through the dialog. An unknown
card, or a product whose manifest lacks `includes.pocket`, produces a host error and no dialog.

### Instances

A Pocket link names an instance with `instance=<instance_id>`, on `add` and `open` alike, for any published card that is
not privileged. An instance is present or not on its own, so a link to an instance the user has yet to add leads to the
dialog even when other instances of its card are present. A host that predates instances drops `instance` from an `add`
and from an `open` that leads to the dialog, so it adds the plain card, and a worker draws every card it publishes as a
plain card too, with its back when the definition declares one.

`instance_id` is 1 to 128 bytes of ASCII letters, digits, `.`, `_`, `~` and `-`, compared byte for byte, so it passes
through links and URLs unchanged. A link whose `instance` breaks that rule, or that names `instance` twice, is refused
with a message. The id is stored with the card and travels in links, QR codes and the card's launch URL, so a product
mints a handle only its own worker can map, never an address, a username or a secret, and the host keeps it out of
telemetry, as it keeps the label out. The collection is kept per device, so the worker maps the same id on every device
the user adds it on.

Before showing the dialog for an instance, the host starts the worker and asks it to label the instance with
`Pocket::label_instance`. The worker answers with a label only for an id its own state ties to the user, and refuses any
other. A refusal, an error, no answer within a time the host sets, or a worker without the method ends the add with a
message and no dialog, whoever wrote the link. The dialog shows the card's title, the label and the card's preview, and
host chrome shows the label wherever it names the card, attributed to the product. The host keeps the label given at the
add. Until the worker draws a new instance, its face is the card's preview.

The worker draws an instance whose id it can no longer resolve as unavailable, on its back too when its card has one,
rather than refusing the render or removing the card.

### Removing a card

The user removes a card in host UI. A product removes one of its own with `remove_card` when the user asks it to, such
as from a "Remove from Pocket" button on its page; offering one is up to the product, and the host cannot check that the
user asked. Either way the card is gone: its cached front and back are discarded, its `Renderer::render` streams end,
its references are dropped, and getting it back means the deeplink flow again. Removing a card that is not present
succeeds. Removing a privileged card fails with `Privileged`, for the product, and is not offered to the user.

### Wire surface

Every frame is addressed by a trait byte and a method byte ([RFC 0028](0028-wire-message-type-byte.md)). Pocket holds
trait `18`, the next after `Renderer` at `17`, with `list_subscribe` as method `0`, `remove_card` as method `1` and
`label_instance` as method `2`, which the host calls on the worker. New Pocket methods append within the trait.

```rust
/// Pocket cards backed by the calling product.
#[wire_trait(id = 18)]
#[crate::service(required_execution = Worker)]
#[async_trait::async_trait]
pub trait Pocket: Send + Sync {
    /// The calling product's cards, whole set on subscribe and on every change.
    #[wire(id = 0)]
    async fn list_subscribe(
        &self,
        cx: &CallContext,
    ) -> Subscription<HostPocketListSubscribeItem, CallError<GenericError>>;

    /// Remove one of the calling product's cards. Idempotent.
    #[wire(id = 1)]
    async fn remove_card(
        &self,
        cx: &CallContext,
        request: HostPocketRemoveCardRequest,
    ) -> Result<HostPocketRemoveCardResponse, CallError<HostPocketRemoveCardError>>;

    /// Label an instance the user is about to add, or refuse it. The host calls it before the add dialog.
    #[wire(host_initiated, id = 2)]
    async fn label_instance(
        &self,
        cx: &CallContext,
        request: ProductPocketLabelInstanceRequest,
    ) -> Result<ProductPocketLabelInstanceResponse, CallError<ProductPocketLabelInstanceError>>;
}
```

```rust
pub struct PocketCard {
    /// Card id, as declared in the product's worker manifest.
    pub card_id: String,
    /// The instance's id; `None` for a plain card.
    pub instance_id: Option<String>,
    /// Placed by the host; cannot be removed.
    pub privileged: bool,
}

pub struct HostPocketListSubscribeItem { pub cards: Vec<PocketCard> }

pub struct HostPocketRemoveCardRequest {
    /// Card id, as declared in the product's worker manifest.
    pub card_id: String,
    /// The instance to remove; `None` names the plain card.
    pub instance_id: Option<String>,
}
pub enum HostPocketRemoveCardError {
    /// The card is privileged.
    Privileged,
    Unknown { reason: String },
}

pub struct ProductPocketLabelInstanceRequest {
    /// Card id, as declared in the product's worker manifest.
    pub card_id: String,
    /// The instance the link names.
    pub instance_id: String,
}

pub struct ProductPocketLabelInstanceResponse {
    /// The instance as the user knows it, such as an account's name.
    pub label: String,
}

pub enum ProductPocketLabelInstanceError {
    /// The product does not tie the instance to the user.
    Refused,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
```

Each payload travels in a versioned envelope like every other method. `list_subscribe` and `remove_card` are at `V2`,
which carries `instance_id` in `PocketCard` and `HostPocketRemoveCardRequest`; `V1` is the same without it. A product
built before instances speaks `V1`, so its list holds plain cards only and its removal names a plain card. A host that
predates `V2` refuses a `V2` call as an unsupported version. `label_instance` starts at `V1`.
`HostPocketRemoveCardResponse` carries no payload of its own: removal has nothing to report beyond success, so its
envelope is the bare variant of the call's version, `V1` or `V2`.

The back adds one variant to the renderer's `RenderContext`, `PocketCardBack { card_id }`, appended after `PocketCard`
so existing contexts keep their encoding.

Instances add two variants, `PocketCardInstance { card_id, instance_id }` for the face and
`PocketCardInstanceBack { card_id, instance_id }` for the back, appended in that order after `PocketCardBack` for the
same reason. A worker on an SDK without them cannot label an instance either, so the host never asks it to draw one.

**Bounds.** `card_id` is a product-supplied identifier and carries the rules chat already applies to its own, through
`normalize_chat_identifier`: trimmed, NFC-normalized, non-empty, at most `CHAT_FIELD_MAX_BYTES` (256) bytes once
normalized, and rejected if it contains characters that let two distinct ids render identically (joiners, variation
selectors, soft hyphens, non-ASCII spaces). Pocket reuses the constant rather than declaring its own, so the modalities
cannot drift; chat acquired these bounds only after hosts had already diverged
([#453](https://github.com/paritytech/trinity-user-agents/pull/453)), which is the outcome stating them here avoids.
Action ids and action payloads are the renderer's, and bounded there. An instance's label is screened as a chat display
name is, through `validate_chat_name`, and is never empty; a label that fails ends the add as a refusal does.

No request names a product: the host knows which worker it is talking to, so a product can neither observe nor remove
another product's cards. A host with no Pocket surface answers `remove_card` with `Unavailable` and interrupts
`list_subscribe` with the same error, which a product reads as an unsupported host rather than as an empty collection.

The expanded card's call comes from the Widget, and `Pocket` is reachable only from the worker. An execution restriction
applies to a whole trait, so the calls are a trait of their own, `23`, with `set_face_shown` as method `0` and
`set_back_shown` as method `1`.

```rust
/// The card a Widget is shown under.
#[wire_trait(id = 23)]
#[crate::service(required_execution = Widget)]
#[async_trait::async_trait]
pub trait ExpandedCard: Send + Sync {
    /// Show or hide the face above the calling Widget. Succeeds when the face is already in that state.
    #[wire(id = 0)]
    async fn set_face_shown(
        &self,
        cx: &CallContext,
        request: HostExpandedCardSetFaceShownRequest,
    ) -> Result<HostExpandedCardSetFaceShownResponse, CallError<HostExpandedCardSetFaceShownError>>;

    /// Turn the card so its back (`true`) or its front (`false`) is up. Succeeds when that side is already up.
    #[wire(id = 1)]
    async fn set_back_shown(
        &self,
        cx: &CallContext,
        request: HostExpandedCardSetBackShownRequest,
    ) -> Result<HostExpandedCardSetBackShownResponse, CallError<HostExpandedCardSetBackShownError>>;
}
```

```rust
pub struct HostExpandedCardSetFaceShownRequest {
    /// `true` brings the face back, `false` moves it out of the way.
    pub shown: bool,
}

pub enum HostExpandedCardSetFaceShownError {
    /// The Widget is not shown under its card right now, such as one the host keeps loaded after the card closed.
    NotPresented,
    /// The user is moving the face or turning the card; the request had no effect.
    UserMoving,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

pub struct HostExpandedCardSetBackShownRequest {
    /// `true` turns the back up, `false` turns the front up.
    pub shown: bool,
}

pub enum HostExpandedCardSetBackShownError {
    /// The Widget is not shown under its card right now, such as one the host keeps loaded after the card closed.
    NotPresented,
    /// The user is turning the card or moving its face; the request had no effect.
    UserMoving,
    /// The card has no back to show right now.
    NoBack,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
```

`HostExpandedCardSetFaceShownResponse` and `HostExpandedCardSetBackShownResponse` are bare `V1`s. Each call acts on the
card above the Widget that made it, so no request names a card. An App or Worker caller is `Denied`, and a host without
expanded cards, or one that cannot turn them, answers `Unsupported`. Both calls answer `UserMoving` while the user is
moving the face or turning the card.

## Trade-offs

- **No product-initiated add.** A product cannot surface a card at the moment it becomes relevant; it has to get the
  user to a deeplink. Accepted: the collection is the user's, and a dialog per card is the consent.
- **The node vocabulary is the renderer's.** A face cannot ask for a node the renderer does not carry, so gaps such as a
  barcode node or aspect-ratio control are argued there and close for every surface at once.
- **The preview can lie.** The approved static face and the live face are both product-authored and nothing ties them
  together. The back is never shown at approval at all. The host can bound the drift by rendering both from the same
  node vocabulary, not by checking content.
- **Card definitions cost manifest budget.** Text records are small; a product with many cards pushes the Worker
  manifest toward the dotNS limit. Only ids, titles, and paths go in the manifest; the trees live in the archive.
- **An instance runs product code before the dialog.** The host asks the worker to label an instance before showing the
  dialog, so adding one needs the worker to start and answer, and tells the product that a link to the instance was
  opened. The preview stays a static file; only the label comes from the worker, and like the live face it is the
  product's claim, which the host screens but does not check.
- **Page parameters and slot cards fall short.** An `open` link's page parameters reach only the expanded card, and only
  when that link opens it. A tap on the face opens the page with `card=` alone, and the card's face and back are drawn
  on `PocketCard { card_id }` and `PocketCardBack { card_id }` without them, so one card still stands for every object.
  A product can instead declare a fixed set of slot cards and keep which object each stands for. Its worker can then
  draw the object on the live face, but the title and preview, which the dialog and host chrome show, cannot name it.
  The number of slots caps the objects that get a card and spends manifest budget. The product also tracks slots across
  adds, removals and reuse, and a link made for a slot opens whatever object the slot stands for by then.
- **Expanded cards need a Widget.** A product with cards but no `widget` executable has faces that do not open. Products
  that want interaction without a WebView use face actions. `Pocket` is reachable only from the worker, so an expanded
  card that wants to offer "Remove from Pocket" has to reach its own worker to do it.
- **The face is moved, not locked.** A Widget cannot keep the face away against the user, so a product that wants the
  whole screen has to tolerate the user pulling the face back.
- **Pocket declares no rendering method of its own.** A `card_render` and `action_subscribe` pair keyed by `card_id` was
  considered and dropped: a face is a body like any other, and a pair per surface costs four wire ids each and gives a
  product one handler per surface to register.
- **`set_back_shown` sets a side, it does not toggle.** A toggle undoes itself when called twice or when it races the
  user's own turn. Naming the side makes every call safe to repeat, as `set_face_shown` is.
- **The side is a context variant, not a render payload.** The render request has to name the side to draw, and a
  variant names it for presses too, so a product needs no action-id convention to tell the sides apart. A product on an
  SDK without the variant refuses back renders and keeps its front.
- **The Pocket link is the host's own URL.** The host recognizes it by its host, `pocket`, without reading any
  product's paths, and the same link works from a product, a QR code or a web page. A reserved path under the
  product's name, such as `/-/pocket`, would take that path from every App. A query parameter on the product's URL was
  rejected because Apps tend to ignore unknown parameters, so an unsupported target would silently open the App with no
  signal that anything was asked for. The cost: a host without a Pocket has no App to fall back to, so it refuses the
  link.

## Considerations

- **Which products back Balance and Scarcity.** Humanity has an owner through RFC 0024. Balance and Scarcity are
  host-rendered today and this RFC assumes the host designates a product for each before iteration 1 ships.
- **Where card definitions live if the manifest budget bites.** The fallback is a single `pocket.json` at the archive
  root listing the cards, with the manifest carrying only `includes.pocket`.
- **Material effects on a face.** Predefined surface effects a card picks, animated by device orientation. Not specified
  here: a privileged card is host-placed, so a host can attach one by card identity, and anything a third-party card
  picks is a [material](materials.md).
- **A tap target that turns the card.** Left for later. It can reuse the same turn. Until then a product can put a
  button on its Widget page instead, since a press on the card itself goes to the worker.
