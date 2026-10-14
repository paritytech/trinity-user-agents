---
title: "Unified Renderer"
owner: "@johnthecat"
status: draft
---

# RFC — Unified Renderer

## Summary

Products draw parts of host surfaces, and the host keeps control of what reaches the screen. `Renderer` is the one
service for it. The product describes the body as a `RendererNode` tree over a closed vocabulary, the host draws the
tree from its own design system inside a frame it controls, and an action inside the tree reaches the product on one
stream. A `RenderContext` on the render request and on the action names the surface and the body. A custom chat message
is one such body, and `Chat` has no rendering pair of its own.

## Motivation

A product has one way to show its own information inside a host surface: a `ChatMessageContent::Custom` message,
rendered through `Chat::custom_message_render` and answered through `Chat::action_subscribe`. The request is keyed by
message, the press arrives beside posted messages and slash commands, and the press item does not say whether the button
was drawn by the host for an `Actions` message or by the product in a tree.

The input modality needs a product to draw a candidate, and the pocket modality a card face. A render and action pair
per surface gives a product one render callback and one action stream per surface for the same tree type.

## Requirements

- **Surface-neutral.** A body is drawn and its actions reported the same way on every surface.
- **Closed.** A product names layouts and tokens from a fixed vocabulary, never markup, stylesheets or URLs.
- **Correlated.** An action carries enough for the product to find the body and the handler without a registry.
- **Live.** A product redraws a displayed body in place, and no stream is open for a body off screen.
- **Framed.** The host draws identity, bounds and dismissal around every body and interprets the tree itself.

## Approach

The design has three parts:

- The `Renderer` service and the `RenderContext` that binds its two methods.
- The `RendererNode` tree.
- The action pipeline from a rendered node to the product.

### Service

`Renderer` is a worker service beside `Chat` in the canonical `truapi` crate, at `api/renderer.rs`. Its types live in
`v01::renderer` and are re-exported through `truapi::latest`.

```rust
/// Where a product-rendered body lives, and the id that names it there.
pub enum RenderContext {
    /// A message in a chat room.
    ChatMessage {
        /// Room the message was posted in.
        room_id: String,
        /// Message id, as returned by `Chat::post_message`.
        message_id: String,
        /// Product-defined discriminator, as stored in `ChatCustomMessage::message_type`.
        message_type: String,
    },
    /// A candidate answered to an input query.
    InputWidget {
        /// Candidate id, as the product answered it.
        candidate_id: String,
    },
    /// A card face in the host's Pocket collection.
    PocketCard {
        /// Card id, as declared in the product's worker manifest.
        card_id: String,
    },
}

/// A body the host needs drawn.
pub struct ProductRendererRenderRequest {
    /// Where the body lives.
    pub context: RenderContext,
    /// Product-defined payload, opaque to the host.
    pub payload: Vec<u8>,
}

/// An action triggered inside a product-rendered body.
pub struct HostRendererActionSubscribeItem {
    /// Where the body lives.
    pub context: RenderContext,
    /// Which action was triggered, as named in the renderer tree.
    pub action_id: String,
    /// Data the node attached to the action. Empty for a `Button` press.
    pub payload: Vec<u8>,
}
```

```rust
/// Product-rendered bodies and the actions triggered inside them.
pub trait Renderer: Send + Sync {
    /// Streams renderer trees for one product-rendered body. Each item
    /// replaces the previous tree. The stream stays open while the body is
    /// displayed so the product can redraw in place.
    fn render(
        &self,
        _cx: &CallContext,
        _request: ProductRendererRenderRequest,
    ) -> Subscription<RendererNode, CallError<GenericError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Subscribe to actions triggered inside this product's rendered bodies.
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostRendererActionSubscribeItem, CallError<GenericError>> {
        Subscription::interrupted(CallError::unavailable())
    }
}
```

`RenderContext` has one variant per surface, holding the ids that surface uses to name a body. The host scopes an id to
the product that minted it, and for `InputWidget` also to the query the candidate answered. A render request and the
actions inside it carry the same context verbatim, so a product correlates by equality. A new surface is a new variant.

A product registers one `render` handler, and the host calls it for every context of a surface the product's manifest
`includes`. A product that draws on more than one surface matches on `context`. Contexts for surfaces the product does
not include are never sent.

### Render Tree

A body is one `RendererNode`. An absent `OptionBool` leaves the default to the host.

```rust
/// A size in logical pixels, SCALE-encoded as `Compact<u64>`.
pub type Size = Compact<u64>;

/// Edge dimensions. `bottom` defaults to `top` and `start` to `end` when absent.
pub struct Dimensions {
    pub top: Size,
    pub end: Size,
    pub bottom: Option<Size>,
    pub start: Option<Size>,
}

/// Typography presets, resolved by the host's design system.
pub enum TypographyStyle {
    HeadlineLarge,
    TitleMediumRegular,
    BodyLargeRegular,
    BodyMediumRegular,
    BodySmallRegular,
}

/// Button emphasis.
pub enum ButtonVariant {
    Primary,
    Secondary,
    /// No background.
    Text,
}

/// Semantic color tokens, resolved by the host's theme.
pub enum ColorToken {
    FgPrimary,
    FgSecondary,
    FgTertiary,
    BgSurfaceMain,
    BgSurfaceContainer,
    BgSurfaceNested,
    FgSuccess,
    FgError,
    FgWarning,
}

/// Placement of content within a `Box`.
pub enum ContentAlignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

/// Cross-axis alignment of `Column` children.
pub enum HorizontalAlignment {
    Start,
    Center,
    End,
}

/// Cross-axis alignment of `Row` children.
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

/// Main-axis distribution of children.
pub enum Arrangement {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Outline of a background or border.
pub enum Shape {
    Rounded(Size),
    Circle,
    Square,
}

pub struct BorderStyle {
    pub width: Size,
    pub color: ColorToken,
    pub shape: Option<Shape>,
}

pub struct Background {
    pub color: ColorToken,
    pub shape: Option<Shape>,
}

/// How a node composites with what is behind it. The values are those common to CSS
/// `mix-blend-mode`, SwiftUI `BlendMode` and Compose `BlendMode`.
pub enum BlendingMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Layout and styling applied to one node.
pub enum Modifier {
    /// Outer spacing.
    Margin(Dimensions),
    /// Inner spacing.
    Padding(Dimensions),
    Background(Background),
    Border(BorderStyle),
    Height(Size),
    Width(Size),
    MinWidth(Size),
    MinHeight(Size),
    FillWidth(bool),
    FillHeight(bool),
    /// 0 is transparent, 255 is opaque.
    Opacity(u8),
    BlendingMode(BlendingMode),
}
```

```rust
pub struct BoxProps {
    pub content_alignment: Option<ContentAlignment>,
}

pub struct ColumnProps {
    pub horizontal_alignment: Option<HorizontalAlignment>,
    pub vertical_arrangement: Option<Arrangement>,
}

pub struct RowProps {
    pub vertical_alignment: Option<VerticalAlignment>,
    pub horizontal_arrangement: Option<Arrangement>,
}

pub struct TextProps {
    pub style: Option<TypographyStyle>,
    pub color: Option<ColorToken>,
}

pub struct ButtonProps {
    /// Button label.
    pub text: String,
    pub variant: Option<ButtonVariant>,
    /// Whether the button accepts presses.
    pub enabled: OptionBool,
    /// Whether the button shows a loading state. A loading button accepts no
    /// presses.
    pub loading: OptionBool,
    /// Action triggered on press. A button without one is inert.
    pub click_action: Option<String>,
}

/// Where image bytes come from. The host fetches them; the tree carries no URL.
pub enum ImageSource {
    /// A Bulletin chain blob, addressed by its CID.
    Bulletin(String),
    /// A file inside the product's executable archive, as a path relative to
    /// the archive root.
    Archive(String),
}

pub enum ImageFit {
    /// The image is not resized.
    None,
    /// Resized to fill the container without preserving the aspect ratio.
    Fill,
    /// Preserves the aspect ratio and fills the container, cutting overflow.
    Cover,
    /// Preserves the aspect ratio and fits inside the container, leaving empty space if needed.
    Contain,
    /// Whichever of `None` or `Contain` yields the smaller image.
    ScaleDown,
}

pub struct ImageProps {
    pub source: ImageSource,
    /// Defaults to `Fill`.
    pub fit: Option<ImageFit>,
}

/// A visual effect. Each variant names one effect and carries its parameters.
pub enum Effect {
    Rainbow,
}

pub struct EffectProps {
    pub effect: Effect,
}

pub struct TextFieldProps {
    /// Current value.
    pub text: String,
    /// Shown when the value is empty.
    pub placeholder: Option<String>,
    pub label: Option<String>,
    /// Whether the field accepts input.
    pub enabled: OptionBool,
    /// Action triggered on every value change, carrying the new value.
    pub value_change_action: Option<String>,
}

/// A node in a product-rendered tree. Container variants recurse through
/// `children`.
pub enum RendererNode {
    /// Draws nothing.
    Nil,
    /// A text run.
    String { text: String },
    /// Generic container.
    Box {
        modifiers: Vec<Modifier>,
        props: BoxProps,
        children: Vec<RendererNode>,
    },
    /// Vertical layout.
    Column {
        modifiers: Vec<Modifier>,
        props: ColumnProps,
        children: Vec<RendererNode>,
    },
    /// Horizontal layout.
    Row {
        modifiers: Vec<Modifier>,
        props: RowProps,
        children: Vec<RendererNode>,
    },
    /// Flexible space.
    Spacer {
        modifiers: Vec<Modifier>,
    },
    /// Styled text.
    Text {
        modifiers: Vec<Modifier>,
        props: TextProps,
        children: Vec<RendererNode>,
    },
    Button {
        modifiers: Vec<Modifier>,
        props: ButtonProps,
        children: Vec<RendererNode>,
    },
    /// Single-line text input.
    TextField {
        modifiers: Vec<Modifier>,
        props: TextFieldProps,
    },
    /// Image, sized by modifiers.
    Image {
        modifiers: Vec<Modifier>,
        props: ImageProps,
    },
    /// Applies its effect to its children.
    Effect {
        props: EffectProps,
        children: Vec<RendererNode>,
    },
}
```

The host draws every node from its own design system.

An `ImageSource` is fetched from the Bulletin IPFS gateway or the product's executable archive. An image that cannot be
fetched draws as empty space. A tree nested deeper than the host's bound is a decode failure of the stream.

### Actions

An action id is product-chosen and opaque to the host. `Button::click_action` and `TextField::value_change_action` are
the action sites.

Action payload by node, as the host sends it in `HostRendererActionSubscribeItem::payload`:

| Node        | Trigger      | `payload`                                      |
| ----------- | ------------ | ---------------------------------------------- |
| `Button`    | Press        | Empty                                          |
| `TextField` | Value change | UTF-8 bytes of the new value, no length prefix |

The host fills `context` from the render request whose tree the node belongs to. Only the current tree of an open
`render` stream is a source of actions. Actions published before the product subscribes are buffered until it does.

### Streams

The host opens a `render` stream when the body comes on screen and closes it when the body leaves; the same body coming
back opens a fresh stream with the same request. Each item replaces the whole tree. A stream that ends cleanly leaves
the last tree on screen. After an error the host shows the product's identity and no body. A product that cannot draw
the body ends the stream with an error.

An open `render` stream is one worker reference in [Worker Lifecycle](worker-lifecycle.md) terms, whichever surface
opened it.

### Chat

`Chat` has no `custom_message_render`. A `Custom` message is rendered through `Renderer::render` with a `ChatMessage`
context and the stored message payload as `payload`. Actions inside its tree arrive on `Renderer::action_subscribe`.
`ChatActionPayload::ActionTriggered` carries only a press on a button the host draws for a
`ChatMessageContent::Actions` message.

## Compatibility

The change is breaking for a product that renders `Custom` chat messages: its render handler is `renderer.onRender`, its
tree-action handler is `renderer.actionSubscribe`, and its trees are `RendererNode`. A product that posts no `Custom`
messages is unaffected. Hosts ship `Renderer` and the `Chat` change together.

## Trade-offs

- Chat products that render custom messages break once.
- A string `surface` field instead of the `RenderContext` enum would move the id set into an untyped payload and lose
  correlation by equality.

## Open questions

- The `Effect` variants beyond `Rainbow`, and the parameters each carries.
- Whether a pocket card drawn through `Renderer` needs a viewport signal beyond the stream closing.
