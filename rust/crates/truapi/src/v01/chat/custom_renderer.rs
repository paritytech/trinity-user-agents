use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Compact, Decode, Encode, OptionBool};

/// A size/dimension value (logical pixels) used across the custom renderer.
///
/// Encoded as a SCALE `Compact<u64>`: the common small values cost a single
/// byte on the wire instead of eight.
pub type Size = Compact<u64>;

#[cfg(feature = "uniffi")]
uniffi::custom_type!(Size, u64, {
    remote,
    lower: |size| size.0,
    try_lift: |size| Ok(Compact(size)),
});

/// An optional boolean with the compact SCALE encoding used by renderer props.
pub type OptionalBool = OptionBool;

#[cfg(feature = "uniffi")]
uniffi::custom_type!(OptionalBool, Option<bool>, {
    remote,
    lower: |value| value.0,
    try_lift: |value| Ok(OptionBool(value)),
});

/// CSS-like dimensions: (top, end, bottom, start).
/// Bottom defaults to top, start defaults to end when `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Dimensions {
    /// Top dimension.
    pub top: Size,
    /// End dimension.
    pub end: Size,
    /// Bottom dimension. Defaults to top when absent.
    pub bottom: Option<Size>,
    /// Start dimension. Defaults to end when absent.
    pub start: Option<Size>,
}

/// Text typography presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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

/// Button style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ButtonVariant {
    /// Emphasized button for the primary action.
    Primary,
    /// De-emphasized button for secondary actions.
    Secondary,
    /// Text-only button without a background.
    Text,
}

/// Semantic color tokens for theming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ColorToken {
    /// Primary foreground (text) color.
    FgPrimary,
    /// Secondary foreground color.
    FgSecondary,
    /// Tertiary foreground color.
    FgTertiary,
    /// Main surface background.
    BgSurfaceMain,
    /// Container surface background.
    BgSurfaceContainer,
    /// Nested surface background.
    BgSurfaceNested,
    /// Foreground color for success states.
    FgSuccess,
    /// Foreground color for error states.
    FgError,
    /// Foreground color for warning states.
    FgWarning,
}

/// 2D content alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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

/// Horizontal alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HorizontalAlignment {
    /// Align to the start edge.
    Start,
    /// Center horizontally.
    Center,
    /// Align to the end edge.
    End,
}

/// Vertical alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum VerticalAlignment {
    /// Align to the top.
    Top,
    /// Center vertically.
    Center,
    /// Align to the bottom.
    Bottom,
}

/// Layout arrangement (like CSS flexbox `justify-content`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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

/// Shape for borders and backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Shape {
    /// Border radius value.
    Rounded {
        /// Border radius.
        radius: Size,
    },
    /// Circular shape.
    Circle,
}

/// Border styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BorderStyle {
    /// Border width.
    pub width: Size,
    /// Border color.
    pub color: ColorToken,
    /// Border shape.
    pub shape: Option<Shape>,
}

/// Background styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Background {
    /// Background color.
    pub color: ColorToken,
    /// Background shape.
    pub shape: Option<Shape>,
}

/// Layout and styling modifiers applied to custom renderer components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Modifier {
    /// Outer spacing.
    Margin(Dimensions),
    /// Inner spacing.
    Padding(Dimensions),
    /// Background fill.
    Background(Background),
    /// Border style.
    Border(BorderStyle),
    /// Fixed height.
    Height {
        /// Fixed height.
        height: Size,
    },
    /// Fixed width.
    Width {
        /// Fixed width.
        width: Size,
    },
    /// Minimum width.
    MinWidth {
        /// Minimum width.
        width: Size,
    },
    /// Minimum height.
    MinHeight {
        /// Minimum height.
        height: Size,
    },
    /// Fill available width.
    FillWidth {
        /// Whether width should fill available space.
        enabled: bool,
    },
    /// Fill available height.
    FillHeight {
        /// Whether height should fill available space.
        enabled: bool,
    },
}

/// Properties for a [`CustomRendererNode::Box`] container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BoxProps {
    /// Content alignment within the box.
    pub content_alignment: Option<ContentAlignment>,
}

/// Properties for a [`CustomRendererNode::Column`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ColumnProps {
    /// Horizontal alignment of children.
    pub horizontal_alignment: Option<HorizontalAlignment>,
    /// Vertical arrangement of children.
    pub vertical_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Row`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RowProps {
    /// Vertical alignment of children.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Horizontal arrangement of children.
    pub horizontal_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Text`] display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TextProps {
    /// Typography preset.
    pub style: Option<TypographyStyle>,
    /// Text color.
    pub color: Option<ColorToken>,
}

/// Properties for a [`CustomRendererNode::Button`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ButtonProps {
    /// Button label text.
    pub text: String,
    /// Button style variant.
    pub variant: Option<ButtonVariant>,
    /// Whether the button is enabled. Absent leaves the default to the host.
    pub enabled: OptionalBool,
    /// Whether the button shows a loading state. Absent leaves the default to the host.
    pub loading: OptionalBool,
    /// Action identifier triggered on click.
    pub click_action: Option<String>,
}

/// Properties for a [`CustomRendererNode::TextField`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TextFieldProps {
    /// Current text value.
    pub text: String,
    /// Placeholder text.
    pub placeholder: Option<String>,
    /// Field label.
    pub label: Option<String>,
    /// Whether the field is enabled. Absent leaves the default to the host.
    pub enabled: OptionalBool,
    /// Action identifier triggered when the value changes.
    pub value_change_action: Option<String>,
}

/// A node in the custom renderer UI tree. Component variants contain recursive
/// `children` fields.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CustomRendererNode {
    /// Empty node.
    Nil,
    /// Raw text string.
    String {
        /// Raw text.
        text: String,
    },
    /// Generic container.
    Box {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Box properties.
        props: BoxProps,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
    /// Vertical layout.
    Column {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Column properties.
        props: ColumnProps,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
    /// Horizontal layout.
    Row {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Row properties.
        props: RowProps,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
    /// Flexible space.
    Spacer {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
    /// Text display.
    Text {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Text properties.
        props: TextProps,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
    /// Interactive button.
    Button {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Button properties.
        props: ButtonProps,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
    /// Text input.
    TextField {
        /// Layout and styling modifiers.
        modifiers: Vec<Modifier>,
        /// Text-field properties.
        props: TextFieldProps,
        /// Child nodes.
        children: Vec<CustomRendererNode>,
    },
}

/// Render work sent by the host when a native custom-message cell appears.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductChatCustomMessageRenderRequest {
    /// Stable identifier used to correlate triggered actions.
    pub message_id: String,
    /// Product-defined discriminator used to select a renderer.
    pub message_type: String,
    /// Stored product-defined message payload.
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Encode)]
    struct RendererWireComponent<P> {
        modifiers: Vec<Modifier>,
        props: P,
        children: Vec<CustomRendererNode>,
    }

    #[derive(Encode)]
    enum RendererWireNode<P> {
        #[codec(index = 3)]
        Column(RendererWireComponent<P>),
    }

    fn renderer_node() -> CustomRendererNode {
        CustomRendererNode::Column {
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
                CustomRendererNode::String {
                    text: "Votes: 1".to_string(),
                },
                CustomRendererNode::Button {
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
            ],
        }
    }

    #[test]
    fn renderer_node_preserves_the_component_wire_shape() {
        let node = renderer_node();
        let CustomRendererNode::Column {
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

    #[cfg(feature = "uniffi")]
    #[test]
    fn renderer_node_round_trips_through_uniffi() {
        let node = renderer_node();
        let ffi = <CustomRendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
        let lifted = <CustomRendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();

        assert_eq!(lifted, node);
    }
}
