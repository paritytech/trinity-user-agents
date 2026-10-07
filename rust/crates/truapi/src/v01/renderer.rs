//! Product-rendered body trees and the contexts that name them.

use parity_scale_codec::{Compact, Decode, Encode, OptionBool};

/// SCALE spells an optional bool as its own type; JSON spells it as an absent
/// or present boolean, which is what the generated client describes.
#[cfg(feature = "runtime")]
mod option_bool_json {
    use parity_scale_codec::OptionBool;
    use serde::{Deserialize, Deserializer};

    /// What an absent JSON field reads as: neither true nor false.
    pub fn absent() -> OptionBool {
        OptionBool(None)
    }

    /// Reads a present JSON boolean into the SCALE optional bool.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<OptionBool, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(OptionBool(Option::<bool>::deserialize(deserializer)?))
    }
}

/// A size in logical pixels, SCALE-encoded as `Compact<u64>`.
pub type Size = Compact<u64>;

#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
uniffi::custom_type!(Size, u64, {
    remote,
    lower: |size| size.0,
    try_lift: |size| Ok(Compact(size)),
});

#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
uniffi::custom_type!(OptionBool, Option<bool>, {
    remote,
    lower: |value| value.0,
    try_lift: |value| Ok(OptionBool(value)),
});

/// Edge dimensions. `bottom` defaults to `top` and `start` to `end` when absent.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct Dimensions {
    /// Top edge.
    pub top: Size,
    /// End edge.
    pub end: Size,
    /// Bottom edge; defaults to `top`.
    pub bottom: Option<Size>,
    /// Start edge; defaults to `end`.
    pub start: Option<Size>,
}

/// Typography presets, resolved by the host's design system.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum TypographyStyle {
    /// Large headline text.
    HeadlineLarge,
    /// Medium title text, regular weight.
    TitleMediumRegular,
    /// Large body text, regular weight.
    BodyLargeRegular,
    /// Medium body text, regular weight.
    BodyMediumRegular,
    /// Small body text, regular weight.
    BodySmallRegular,
}

/// Button emphasis.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ButtonVariant {
    /// Emphasized button for the primary action.
    Primary,
    /// De-emphasized button for secondary actions.
    Secondary,
    /// Text-only button without a background.
    Text,
}

/// Semantic color tokens, resolved by the host's theme.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ColorToken {
    /// Primary foreground.
    FgPrimary,
    /// Secondary foreground.
    FgSecondary,
    /// Tertiary foreground.
    FgTertiary,
    /// Main surface background.
    BgSurfaceMain,
    /// Container surface background.
    BgSurfaceContainer,
    /// Nested surface background.
    BgSurfaceNested,
    /// Foreground for success states.
    FgSuccess,
    /// Foreground for error states.
    FgError,
    /// Foreground for warning states.
    FgWarning,
}

/// Placement of content within a `Box`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ContentAlignment {
    /// Top edge, start side.
    TopStart,
    /// Top edge, horizontally centered.
    TopCenter,
    /// Top edge, end side.
    TopEnd,
    /// Vertically centered, start side.
    CenterStart,
    /// Centered on both axes.
    Center,
    /// Vertically centered, end side.
    CenterEnd,
    /// Bottom edge, start side.
    BottomStart,
    /// Bottom edge, horizontally centered.
    BottomCenter,
    /// Bottom edge, end side.
    BottomEnd,
}

/// Cross-axis alignment of `Column` children.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum HorizontalAlignment {
    /// Align to the start edge.
    Start,
    /// Center horizontally.
    Center,
    /// Align to the end edge.
    End,
}

/// Cross-axis alignment of `Row` children.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum VerticalAlignment {
    /// Align to the top.
    Top,
    /// Center vertically.
    Center,
    /// Align to the bottom.
    Bottom,
}

/// Main-axis distribution of children.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum Arrangement {
    /// Pack children at the start.
    Start,
    /// Pack children at the end.
    End,
    /// Pack children in the center.
    Center,
    /// Distribute with space between children.
    SpaceBetween,
    /// Distribute with space around each child.
    SpaceAround,
    /// Distribute with equal space between and around children.
    SpaceEvenly,
}

/// Outline of a background or border.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(tag = "tag", content = "value"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum Shape {
    /// Rounded corners with the given radius.
    Rounded(Size),
    /// Circular shape.
    Circle,
    /// Square corners.
    Square,
}

/// Border styling.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct BorderStyle {
    /// Border width.
    pub width: Size,
    /// Border color.
    pub color: ColorToken,
    /// Border shape.
    pub shape: Option<Shape>,
}

/// Background styling.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct Background {
    /// Background color.
    pub color: ColorToken,
    /// Background shape.
    pub shape: Option<Shape>,
}

/// How a node composites with what is behind it. The values are those common
/// to CSS `mix-blend-mode`, SwiftUI `BlendMode` and Compose `BlendMode`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum BlendingMode {
    /// Source over destination.
    Normal,
    /// Multiplies source and destination.
    Multiply,
    /// Inverse multiply.
    Screen,
    /// Multiply or screen depending on the destination.
    Overlay,
    /// Darker of source and destination.
    Darken,
    /// Lighter of source and destination.
    Lighten,
    /// Brightens the destination to reflect the source.
    ColorDodge,
    /// Darkens the destination to reflect the source.
    ColorBurn,
    /// Multiply or screen depending on the source.
    HardLight,
    /// Darken or lighten depending on the source.
    SoftLight,
    /// Absolute difference.
    Difference,
    /// Difference with lower contrast.
    Exclusion,
    /// Source hue with destination saturation and luminosity.
    Hue,
    /// Source saturation with destination hue and luminosity.
    Saturation,
    /// Source hue and saturation with destination luminosity.
    Color,
    /// Source luminosity with destination hue and saturation.
    Luminosity,
}

/// Layout and styling applied to one node.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(tag = "tag", content = "value"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum Modifier {
    /// Outer spacing.
    Margin(Dimensions),
    /// Inner spacing.
    Padding(Dimensions),
    /// Background fill.
    Background(Background),
    /// Border.
    Border(BorderStyle),
    /// Fixed height.
    Height(Size),
    /// Fixed width.
    Width(Size),
    /// Minimum width.
    MinWidth(Size),
    /// Minimum height.
    MinHeight(Size),
    /// Fill the available width.
    FillWidth(bool),
    /// Fill the available height.
    FillHeight(bool),
    /// 0 is transparent, 255 is opaque.
    Opacity(u8),
    /// Compositing mode against what is behind the node.
    BlendingMode(BlendingMode),
}

/// Properties of a `Box`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct BoxProps {
    /// Placement of content within the box.
    pub content_alignment: Option<ContentAlignment>,
}

/// Properties of a `Column`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ColumnProps {
    /// Cross-axis alignment of children.
    pub horizontal_alignment: Option<HorizontalAlignment>,
    /// Main-axis distribution of children.
    pub vertical_arrangement: Option<Arrangement>,
}

/// Properties of a `Row`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct RowProps {
    /// Cross-axis alignment of children.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Main-axis distribution of children.
    pub horizontal_arrangement: Option<Arrangement>,
}

/// Properties of a `Text`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct TextProps {
    /// Typography preset.
    pub style: Option<TypographyStyle>,
    /// Text color.
    pub color: Option<ColorToken>,
}

/// Properties of a `Button`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ButtonProps {
    /// Button label.
    pub text: String,
    /// Button emphasis.
    pub variant: Option<ButtonVariant>,
    /// Whether the button accepts presses. Absent leaves the default to the host.
    #[cfg_attr(
        feature = "runtime",
        serde(
            default = "option_bool_json::absent",
            deserialize_with = "option_bool_json::deserialize"
        )
    )]
    pub enabled: OptionBool,
    /// Whether the button shows a loading state. A loading button accepts no
    /// presses. Absent leaves the default to the host.
    #[cfg_attr(
        feature = "runtime",
        serde(
            default = "option_bool_json::absent",
            deserialize_with = "option_bool_json::deserialize"
        )
    )]
    pub loading: OptionBool,
    /// Action triggered on press. A button without one is inert.
    pub click_action: Option<String>,
}

/// Where image bytes come from. The host fetches them; the tree carries no URL.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(tag = "tag", content = "value"))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ImageSource {
    /// A Bulletin chain blob, addressed by its CID.
    Bulletin(String),
    /// A file inside the product's executable archive, as a path relative to
    /// the archive root.
    Archive(String),
}

/// How an image meets the box its modifiers size.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ImageFit {
    /// The image is not resized.
    None,
    /// Resized to fill the container without preserving the aspect ratio.
    Fill,
    /// Preserves the aspect ratio and fills the container, cutting overflow.
    Cover,
    /// Preserves the aspect ratio and fits inside the container, leaving empty
    /// space if needed.
    Contain,
    /// Whichever of `None` or `Contain` yields the smaller image.
    ScaleDown,
}

/// Properties of an `Image`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ImageProps {
    /// Where the image bytes come from.
    pub source: ImageSource,
    /// Defaults to `Fill`.
    pub fit: Option<ImageFit>,
}

/// A visual effect. Each variant names one effect and carries its parameters.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum Effect {
    /// Animated rainbow tint over the children.
    Rainbow,
}

/// Properties of an `Effect`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct EffectProps {
    /// The effect applied to the children.
    pub effect: Effect,
}

/// Properties of a `TextField`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct TextFieldProps {
    /// Current value.
    pub text: String,
    /// Shown when the value is empty.
    pub placeholder: Option<String>,
    /// Field label.
    pub label: Option<String>,
    /// Whether the field accepts input. Absent leaves the default to the host.
    #[cfg_attr(
        feature = "runtime",
        serde(
            default = "option_bool_json::absent",
            deserialize_with = "option_bool_json::deserialize"
        )
    )]
    pub enabled: OptionBool,
    /// Action triggered on every value change. The action carries the new
    /// value as UTF-8 bytes, with no length prefix.
    pub value_change_action: Option<String>,
}

/// A node in a product-rendered tree. Container variants recurse through
/// `children`.
#[cfg_attr(feature = "runtime", derive(serde::Deserialize))]
#[cfg_attr(feature = "runtime", serde(tag = "tag", content = "value"))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum RendererNode {
    /// Draws nothing.
    Nil,
    /// A text run.
    String {
        /// Raw text.
        text: String,
    },
    /// Generic container.
    Box {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Box properties.
        props: BoxProps,
        /// Child nodes.
        children: Vec<RendererNode>,
    },
    /// Vertical layout.
    Column {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Column properties.
        props: ColumnProps,
        /// Child nodes.
        children: Vec<RendererNode>,
    },
    /// Horizontal layout.
    Row {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Row properties.
        props: RowProps,
        /// Child nodes.
        children: Vec<RendererNode>,
    },
    /// Flexible space.
    Spacer {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
    },
    /// Styled text.
    Text {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Text properties.
        props: TextProps,
        /// Child nodes.
        children: Vec<RendererNode>,
    },
    /// Interactive button.
    Button {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Button properties.
        props: ButtonProps,
        /// Child nodes.
        children: Vec<RendererNode>,
    },
    /// Single-line text input.
    TextField {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Text-field properties.
        props: TextFieldProps,
    },
    /// Image, sized by modifiers.
    Image {
        /// Layout and styling.
        modifiers: Vec<Modifier>,
        /// Image properties.
        props: ImageProps,
    },
    /// Applies its effect to its children.
    Effect {
        /// Effect properties.
        props: EffectProps,
        /// Child nodes.
        children: Vec<RendererNode>,
    },
}

/// Where a product-rendered body lives, and the id that names it there.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum RenderContext {
    /// A message in a chat room.
    ChatMessage {
        /// Room the message was posted in.
        room_id: String,
        /// Message id, as returned by `Chat::post_message`.
        message_id: String,
        /// Product-defined discriminator, as stored in
        /// `ChatCustomMessage::message_type`.
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ProductRendererRenderRequest {
    /// Where the body lives.
    pub context: RenderContext,
    /// Product-defined payload, opaque to the host.
    pub payload: Vec<u8>,
}

/// An action triggered inside a product-rendered body.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostRendererActionSubscribeItem {
    /// Where the body lives.
    pub context: RenderContext,
    /// Which action was triggered, as named in the renderer tree.
    pub action_id: String,
    /// Data the node attached to the action. A `Button` press carries an
    /// empty payload; a `TextField` value change carries the UTF-8 bytes of
    /// the new value, with no length prefix.
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Encode)]
    struct RendererWireComponent<P> {
        modifiers: Vec<Modifier>,
        props: P,
        children: Vec<RendererNode>,
    }

    #[derive(Encode)]
    enum RendererWireNode<P> {
        #[codec(index = 3)]
        Column(RendererWireComponent<P>),
    }

    fn renderer_node() -> RendererNode {
        RendererNode::Column {
            modifiers: vec![Modifier::Padding(Dimensions {
                top: Compact(12),
                end: Compact(8),
                bottom: None,
                start: Some(Compact(4)),
            })],
            props: ColumnProps {
                horizontal_alignment: Some(HorizontalAlignment::Center),
                vertical_arrangement: Some(Arrangement::SpaceBetween),
            },
            children: vec![
                RendererNode::String {
                    text: "Votes: 1".to_string(),
                },
                RendererNode::Button {
                    modifiers: Vec::new(),
                    props: ButtonProps {
                        text: "Vote".to_string(),
                        variant: Some(ButtonVariant::Primary),
                        enabled: OptionBool(Some(true)),
                        loading: OptionBool(None),
                        click_action: Some("vote".to_string()),
                    },
                    children: Vec::new(),
                },
                RendererNode::Spacer {
                    modifiers: vec![Modifier::Opacity(128)],
                },
                RendererNode::Image {
                    modifiers: vec![Modifier::Width(Compact(40))],
                    props: ImageProps {
                        source: ImageSource::Bulletin("bafy".to_string()),
                        fit: Some(ImageFit::Cover),
                    },
                },
                RendererNode::Effect {
                    props: EffectProps {
                        effect: Effect::Rainbow,
                    },
                    children: vec![RendererNode::Nil],
                },
            ],
        }
    }

    #[test]
    fn column_preserves_the_component_wire_shape() {
        let node = renderer_node();
        let RendererNode::Column {
            modifiers,
            props,
            children,
        } = node.clone()
        else {
            unreachable!();
        };
        let wire = RendererWireNode::Column(RendererWireComponent {
            modifiers,
            props,
            children,
        });

        assert_eq!(node.encode(), wire.encode());
    }

    #[test]
    fn variants_encode_only_the_fields_they_carry() {
        let spacer = RendererNode::Spacer {
            modifiers: Vec::new(),
        };
        assert_eq!(spacer.encode(), vec![5, 0]);

        let effect = RendererNode::Effect {
            props: EffectProps {
                effect: Effect::Rainbow,
            },
            children: Vec::new(),
        };
        assert_eq!(effect.encode(), vec![10, 0, 0]);
    }

    #[test]
    fn tree_round_trips_through_scale() {
        let node = renderer_node();
        let decoded = RendererNode::decode(&mut node.encode().as_slice()).unwrap();
        assert_eq!(decoded, node);
    }

    #[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
    #[test]
    fn tree_round_trips_through_uniffi() {
        let node = renderer_node();
        let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
        let lifted = <RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();

        assert_eq!(lifted, node);
    }
}
