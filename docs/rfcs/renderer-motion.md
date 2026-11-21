---
title: "Renderer gestures and driven motion"
owner: "@ERussel"
status: draft
---

# RFC — Renderer gestures and driven motion

## Summary

A product-drawn body can declare a pan gesture that moves a value the host keeps on the device, and node properties
computed from that value every frame. The `Pan`, `Drive` and `Perspective` modifiers carry both. A pan's outcome reaches
the product as a `PanEvent` on the [Unified Renderer](unified-renderer.md)'s action stream, and the product owns every
state the gesture changes. A Pocket card turn becomes an ordinary product tree, and the card back leaves the protocol.

## Motivation

Turning a Pocket card over is the one motion a product can get today, and the host owns all of it. The back is a second
body on its own `PocketCardBack` context, with its own render stream and its own stored face. The host keeps which side
is up, implements the turn physics on each platform, filters back presses for SDKs that predate the context, and serves
`ExpandedCard::set_back_shown` so the Widget can ask for a side. Every rule about a card's sides lives in the host,
while the content of both sides is the product's.

A product cannot drive any other motion, because a frame cannot wait for the product. A worker reaches the core
asynchronously, today across a process boundary, so a tree redrawn per finger move would lag the finger. What a product
needs is to describe how its tree moves under a gesture and hear where the gesture ended.

## Requirements

- **Local:** the frame path, from finger to drawn frame, never waits on the product.
- **Product-owned:** the product owns all state a gesture changes, and the host keeps only the value of a gesture in
  flight.
- **Generic:** the host draws motion without knowing what it means, such as a card or a side.
- **Compatible:** no new action shape or render context reaches an SDK that cannot decode it, and a product can ask
  whether a host draws motion.
- **Consistent:** the same gesture comes to rest on the same value on every host.

## Approach

The design has five parts:

- The `Pan`, `Drive` and `Perspective` modifiers.
- How a pan's value moves, and how a new tree reconciles with it.
- `PanEvent` on the action stream.
- What the host decides about a pan on its surfaces.
- The Pocket card turn as a product tree, and the removal of the card back.

### Modifiers

`Pan`, `Drive` and `Perspective` are appended to `v01::Modifier`, so every existing variant keeps its wire index.
Fractional numbers are IEEE 754 single-precision bit patterns, as material parameters are.

```rust
/// Layout, styling and motion applied to one node.
pub enum Modifier {
    /// Outer spacing.
    Margin(Dimensions),
    /// Inner spacing, around the content: a fixed or minimum size is the content's.
    Padding(Dimensions),
    /// Fill behind the node.
    Background(Background),
    /// Outline around the node.
    Border(BorderStyle),
    /// Fixed height.
    Height(Size),
    /// Fixed width.
    Width(Size),
    /// Minimum width.
    MinWidth(Size),
    /// Minimum height.
    MinHeight(Size),
    /// Whether the node takes its parent's full width.
    FillWidth(bool),
    /// Whether the node takes its parent's full height.
    FillHeight(bool),
    /// 0 is transparent, 255 is opaque.
    Opacity(u8),
    /// How the node composites with what is behind it.
    BlendingMode(BlendingMode),
    /// Drags on this node along one axis move a value the host keeps.
    Pan(Pan),
    /// A property of this node, computed every frame from a pan's value.
    Drive(Drive),
    /// Distance of the viewer from children rotated in 3D, in logical pixels.
    Perspective(Size),
}

/// A drag that moves a host-kept value, which comes to rest on whole steps.
pub struct Pan {
    /// Unique in the tree. Drives and the host's state refer to it.
    pub id: String,
    /// Axis the drag follows.
    pub axis: PanAxis,
    /// Per mille of the node's size along `axis` that moves the value one step. Negative turns a leftward or upward
    /// drag into a positive step.
    pub distance: i32,
    /// The resting step the product declares. Applied when it changes between trees.
    pub value: i32,
    /// Steps one drag may move from where it began. Absent is unbounded.
    pub max_steps_per_gesture: Option<u32>,
    /// Drives read the value modulo this many steps. Absent does not wrap.
    pub period: Option<u32>,
    /// How a change of `value` between trees is animated. Absent jumps.
    pub animation: Option<Animation>,
    /// Action the host reports `PanEvent`s on. Absent reports nothing.
    pub action: Option<String>,
    /// Name of the accessibility action that steps the value by one. Absent offers none.
    pub label: Option<String>,
}

/// The axis a pan follows.
pub enum PanAxis {
    /// Sideways drags.
    Horizontal,
    /// Up and down drags.
    Vertical,
}

/// A property driven by a pan's value.
pub struct Drive {
    /// A `Pan::id` in the same tree.
    pub source: String,
    /// The property the keyframes set.
    pub property: DrivenProperty,
    /// Ascending in `at`, over `[0, period)` when the pan wraps.
    pub keyframes: Vec<Keyframe>,
}

/// A node property a drive can set.
pub enum DrivenProperty {
    /// Rotation about the horizontal axis, in degrees.
    RotationX,
    /// Rotation about the vertical axis, in degrees.
    RotationY,
    /// Rotation in the plane, in degrees.
    RotationZ,
    /// Scale factor about the node's center.
    Scale,
    /// 0 to 1, multiplied with `Modifier::Opacity`.
    Opacity,
    /// Horizontal offset, in logical pixels.
    TranslationX,
    /// Vertical offset, in logical pixels.
    TranslationY,
}

/// The value of a driven property at one pan value.
pub struct Keyframe {
    /// The pan value, as an IEEE 754 single bit pattern.
    pub at: u32,
    /// The property value at `at`, as an IEEE 754 single bit pattern.
    pub value: u32,
    /// How the property reaches `value` from the previous keyframe.
    pub curve: KeyframeCurve,
}

/// The path between two keyframes.
pub enum KeyframeCurve {
    /// A straight line from the previous keyframe.
    Linear,
    /// Holds the previous value until `at`, then jumps.
    Step,
}

/// How the host moves a pan to a value a new tree declares.
pub enum Animation {
    /// A spring that settles within `duration_ms`.
    Spring {
        /// Time to settle, in milliseconds.
        duration_ms: u32,
        /// Overshoot, in per mille. 0 does not overshoot.
        bounce_per_mille: u16,
    },
    /// An ease in and out over `duration_ms`.
    Ease {
        /// Duration, in milliseconds.
        duration_ms: u32,
    },
}
```

### Pan value

A pan's value is a real number of steps, and rest positions are integers. A drag of `distance` along `axis` moves it by
one step. On release the host projects the fling, picks the nearest integer within `max_steps_per_gesture` of where the
drag began, and springs there. The release physics are each host's own, native on each platform, and held together by
test vectors in the core: drag sequences with a release velocity and the rest value they reach, and keyframe
evaluations. Both hosts run them.

A drive evaluates its keyframes at the value modulo `period`. The host knows nothing of what a step means. A card that
turns by half a turn per step drives the front's `RotationY` from 0° at 0 to 360° at 2, the back's from -180° to 180°,
and swaps their `Opacity` with `Step` keyframes at 0.5 and 1.5, with `period: 2`.

A new tree reconciles with the value on the device by these rules:

- The host animates to a tree's `value` only when it differs from the previous tree's value for that id. A tree sent
  before a drag settled, still declaring the old value, changes nothing.
- A change that arrives while a finger holds the pan waits until the finger lets go, and the following `Settled` tells
  the product where the value came to rest.
- State lives per render stream. A new stream starts from the declared value, and a tree without the id drops its state.
- Of several trees within one frame, the host applies the newest.
- A node whose effective opacity is 0 takes no touches, so a hidden face cannot be pressed.

The core checks each tree before a host draws it, beside its check of inline images. A drive naming an unknown pan, a
repeated pan id, unsorted keyframes, or a zero `period` or `distance` is dropped, as is anything past 4 pans, 32 drives
or 16 keyframes per drive. A bad tree draws without that motion rather than failing the stream.

### PanEvent

A pan reports on its `action` as any action does, in `HostRendererActionSubscribeItem::payload`, SCALE-encoded.

```rust
/// What a pan reports, SCALE-encoded in `HostRendererActionSubscribeItem::payload`.
pub enum PanEvent {
    /// A finger took the pan. The value may move before the next event.
    Began,
    /// The value came to rest, after a drag, an accessibility step or an animation.
    Settled {
        /// The rest value, in steps.
        value: i32,
    },
    /// Something else took the press before the value moved.
    Cancelled,
}
```

The payload reaches only the product that declared the pan, so an SDK that does not know `PanEvent` never receives one.
A new action variant would reach every subscriber and end the action stream of any SDK that cannot decode it.

A host that draws motion answers `true` to `HostFeatureSupportedRequest::RendererMotion`, appended to that enum, as it
answers `Materials`. A product asks before sending a tree with `Pan`, `Drive` or `Perspective`, because a host that does
not know the modifiers fails to decode the tree.

### Host rules

- A horizontal pan claims a drag on its first move past the touch slop when the move is mostly sideways, and leaves
  other drags to the host, such as a list scroll.
- A host may decline a pan on a surface, or on one axis where the host owns that axis, such as the expanded card's pull
  to close or the Pocket collection's scroll. The tree then draws at its declared value.
- With reduced motion on, the host jumps or cross-fades instead of animating, and reports the same events.
- A pan with a `label` is offered to the screen reader as an action that steps the value by one and reports `Settled`.

### Pocket card turn

A card draws both its faces in one `PocketCard` tree, stacked under one pan and driven as [Pan value](#pan-value) shows. The host
does not know the card has a back.

The worker owns the side as one integer and is the only source of trees. Three things can change the side, and each
reaches the worker as an event:

| Source               | What the worker receives                        | What the host draws                                    |
| -------------------- | ----------------------------------------------- | ------------------------------------------------------ |
| A drag on the card   | `Began`, then `Settled { value }` on the pan    | The turn as the finger moves, before the worker hears  |
| A button on the face | The button's `click_action`                     | The pan's `animation` to the value the redraw declares |
| The Widget page      | A change on its `localStorage.subscribe` stream | The pan's `animation` to the value the redraw declares |

The page and the worker share the product's storage namespace
([storage subscriptions](storage-subscriptions-pending-operations.md)), so the page asks for a side by writing a key the
worker subscribes to, and reads the outcome from the same key. The worker writes the key whenever it changes the side,
and ignores a change equal to the side it holds, so its own writes do not redraw. A finger wins over both other sources,
because the host holds a value change until the finger lets go and the `Settled` that follows is the last write. Which
side a card opens on is the product's choice.

### Card back removal

The host stops drawing, storing and turning a card back, and the protocol loses every item that served it:

| Item                                         | What happens                                                           |
| -------------------------------------------- | ---------------------------------------------------------------------- |
| `RenderContext::PocketCardBack`              | Never sent; stays in the v0.2 enum so `ChatRoomFooter` keeps its index |
| `ExpandedCard::set_back_shown` and its types | Removed; `ExpandedCard` keeps `set_face_shown`                         |
| A card's stored back face                    | Removed, and stored backs are deleted                                  |
| The back press filter for older SDKs         | Removed                                                                |
| The manifest's `back` field                  | Removed from `PocketCardDefinition`                                    |

[Pocket modality](pocket-modality.md), [Unified Renderer](unified-renderer.md) and
[Expanded card layering](expanded-card-layering.md) describe a card with one face.

## Compatibility

Removing `ExpandedCard::set_back_shown`, which `@parity/truapi` 0.31 and 0.32 publish, is breaking, as is removing
`PocketCardSide` from the native bindings. No host on main draws a back, so no product loses a back it shows today. The modifiers and `PanEvent` are
additive: a product that sends none is unaffected, and one that sends them to a host without `RendererMotion` keeps the
card's cached face, as after any render error.

## Trade-offs

- Both faces ride in every tree, inline images included. Hosts cache decoded inline images by their bytes, so a redraw
  decodes nothing.
- A turn the page asks for crosses storage, the worker and a redraw before its first frame. A host call that sets a
  pan's value directly would be faster, and is held back until the storage path proves too slow.
- The host draws no card edge while the card is side-on. A product that wants one draws it as a third driven child.
- Release physics are two native implementations. Physics in the core would be one, at the cost of a core call on every
  frame; the shared vectors keep the two from drifting.
- Per-frame trees from the product were rejected, because the worker cannot answer within a frame.
