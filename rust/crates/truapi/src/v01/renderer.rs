//! Product-rendered body trees and the contexts that name them.

use parity_scale_codec::{Compact, Decode, Encode, OptionBool};

/// SCALE spells an optional bool as its own type; JSON spells it as an absent
/// or present boolean, which is what the generated client describes.
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

/// SCALE carries bytes as they are; JSON carries them as the generated client
/// spells them, a hex string with a `0x` prefix.
mod hex_bytes_json {
	use serde::{Deserialize, Deserializer, de::Error};

	/// Reads a hex string into bytes.
	pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
	where
		D: Deserializer<'de>,
	{
		let text = String::deserialize(deserializer)?;
		let digits = text
			.strip_prefix("0x")
			.ok_or_else(|| D::Error::custom("hex bytes need a 0x prefix"))?;
		hex::decode(digits).map_err(D::Error::custom)
	}
}

/// A size in logical pixels, SCALE-encoded as `Compact<u64>`.
pub type Size = Compact<u64>;

#[cfg(not(target_arch = "wasm32"))]
uniffi::custom_type!(Size, u64, {
	remote,
	lower: |size| size.0,
	try_lift: |size| Ok(Compact(size)),
});

#[cfg(not(target_arch = "wasm32"))]
uniffi::custom_type!(OptionBool, Option<bool>, {
	remote,
	lower: |value| value.0,
	try_lift: |value| Ok(OptionBool(value)),
});

/// Edge dimensions. `bottom` defaults to `top` and `start` to `end` when absent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ButtonVariant {
	/// Emphasized button for the primary action.
	Primary,
	/// De-emphasized button for secondary actions.
	Secondary,
	/// Text-only button without a background.
	Text,
}

/// Semantic color tokens, resolved by the host's theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HorizontalAlignment {
	/// Align to the start edge.
	Start,
	/// Center horizontally.
	Center,
	/// Align to the end edge.
	End,
}

/// Cross-axis alignment of `Row` children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum VerticalAlignment {
	/// Align to the top.
	Top,
	/// Center vertically.
	Center,
	/// Align to the bottom.
	Bottom,
}

/// Main-axis distribution of children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Arrangement {
	/// Pack children at the start.
	Start,
	/// Pack children at the end.
	End,
	/// Pack children in the center.
	Center,
	/// Distribute with space between children.
	SpaceBetween,
	/// Distribute with space around each child: half a gap at each end.
	SpaceAround,
	/// Distribute with equal space between and around children.
	SpaceEvenly,
}

/// Outline of a background or border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Shape {
	/// Rounded corners with the given radius.
	Rounded(Size),
	/// Circular shape.
	Circle,
	/// Square corners.
	Square,
}

/// Border styling. Drawn inside the node, so it never covers a neighbour.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct BorderStyle {
	/// Border width.
	pub width: Size,
	/// Border color.
	pub color: ColorToken,
	/// Border shape.
	pub shape: Option<Shape>,
}

/// Background styling. A shaped background also clips the node's content to its shape.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Background {
	/// Background color.
	pub color: ColorToken,
	/// Background shape.
	pub shape: Option<Shape>,
}

/// How a node composites with what is behind it. The values are those common
/// to CSS `mix-blend-mode`, SwiftUI `BlendMode` and Compose `BlendMode`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Modifier {
	/// Outer spacing.
	Margin(Dimensions),
	/// Inner spacing, around the content: a fixed or minimum size is the content's.
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
	/// Fill the available height. In a `Row` with no height of its own, that is its tallest
	/// child's.
	FillHeight(bool),
	/// 0 is transparent, 255 is opaque.
	Opacity(u8),
	/// Compositing mode against what is behind the node.
	BlendingMode(BlendingMode),
}

/// Properties of a `Box`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct BoxProps {
	/// Placement of content within the box. Top start when absent.
	pub content_alignment: Option<ContentAlignment>,
}

/// Properties of a `Column`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ColumnProps {
	/// Cross-axis alignment of children.
	pub horizontal_alignment: Option<HorizontalAlignment>,
	/// Main-axis distribution of the height the column was given. Children
	/// have no gap between them, and a column with no height of its own hugs them.
	pub vertical_arrangement: Option<Arrangement>,
}

/// Properties of a `Row`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct RowProps {
	/// Cross-axis alignment of children.
	pub vertical_alignment: Option<VerticalAlignment>,
	/// Main-axis distribution of the width the row was given. Children have no
	/// gap between them, and a row with no width of its own hugs them.
	pub horizontal_arrangement: Option<Arrangement>,
}

/// Properties of a `Text`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TextProps {
	/// Typography preset.
	pub style: Option<TypographyStyle>,
	/// Text color.
	pub color: Option<ColorToken>,
}

/// Properties of a `Button`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ButtonProps {
	/// Button label.
	pub text: String,
	/// Button emphasis.
	pub variant: Option<ButtonVariant>,
	/// Whether the button accepts presses. Absent leaves the default to the host.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub enabled: OptionBool,
	/// Whether the button shows a loading state. A loading button accepts no
	/// presses. Absent leaves the default to the host.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub loading: OptionBool,
	/// Action triggered on press. A button without one is inert.
	pub click_action: Option<String>,
}

/// Where image bytes come from: a place the host fetches them from, or the
/// tree itself. The tree carries no URL. Fetched bytes are PNG, JPEG, WebP or
/// SVG; inline bytes leave out SVG.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ImageSource {
	/// A Bulletin chain blob, addressed by its CID.
	Bulletin(String),
	/// A file inside the product's executable archive, as a path relative to
	/// the archive root.
	Archive(String),
	/// An image carried in the tree itself: PNG, JPEG or WebP, at most 64 KiB.
	Inline(#[serde(deserialize_with = "hex_bytes_json::deserialize")] Vec<u8>),
}

/// How an image meets the box its modifiers size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ImageFit {
	/// The image is not resized: one image pixel to a point.
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ImageProps {
	/// Where the image bytes come from.
	pub source: ImageSource,
	/// Defaults to `Fill`.
	pub fit: Option<ImageFit>,
}

/// A visual effect. Each variant names one effect and carries its parameters.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Effect {
	/// Animated rainbow tint over the children.
	Rainbow,
}

/// Properties of an `Effect`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct EffectProps {
	/// The effect applied to the children.
	pub effect: Effect,
}

/// Properties of a `TextField`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TextFieldProps {
	/// Current value.
	pub text: String,
	/// Shown when the value is empty.
	pub placeholder: Option<String>,
	/// Field label.
	pub label: Option<String>,
	/// Whether the field accepts input. Absent leaves the default to the host.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub enabled: OptionBool,
	/// Action triggered on every value change. The action carries the new
	/// value as UTF-8 bytes, with no length prefix.
	pub value_change_action: Option<String>,
}

/// A node in a product-rendered tree. Container variants recurse through
/// `children`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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
	/// Flexible space: shares out what its stack has left, unless sized along
	/// the stack's axis.
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
	/// Interactive button, as wide as the space it is given unless sized.
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
	/// Image, sized by modifiers. Without any it takes its natural size, one
	/// image pixel to a point.
	Image {
		/// Layout and styling.
		modifiers: Vec<Modifier>,
		/// Image properties.
		props: ImageProps,
	},
	/// Applies its effect to its children. As large as its children, which it
	/// places at the top start like a `Box`.
	Effect {
		/// Effect properties.
		props: EffectProps,
		/// Child nodes.
		children: Vec<RendererNode>,
	},
}

/// Where a product-rendered body lives, and the id that names it there.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
	/// The back of a card in the host's Pocket collection, for a card whose
	/// definition declares `back`.
	PocketCardBack {
		/// Card id, as declared in the product's worker manifest.
		card_id: String,
	},
}

/// A body the host needs drawn.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductRendererRenderRequest {
	/// Where the body lives.
	pub context: RenderContext,
	/// Product-defined payload, opaque to the host.
	pub payload: Vec<u8>,
}

/// An action triggered inside a product-rendered body.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
mod tests;
