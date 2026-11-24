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

	/// Reads a hex string into bytes, or into a fixed array of exactly as many.
	pub fn deserialize<'de, D, Bytes>(deserializer: D) -> Result<Bytes, D::Error>
	where
		D: Deserializer<'de>,
		Bytes: TryFrom<Vec<u8>>,
	{
		let text = String::deserialize(deserializer)?;
		let digits = text
			.strip_prefix("0x")
			.ok_or_else(|| D::Error::custom("hex bytes need a 0x prefix"))?;
		let bytes = hex::decode(digits).map_err(D::Error::custom)?;
		Bytes::try_from(bytes).map_err(|_| D::Error::custom("wrong number of hex bytes"))
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

/// A background in a colour given by value. Drawn and clipped as
/// [`Background`] is.
///
/// Every colour given by value in this module is `0xRRGGBBAA`: sRGB, the alpha
/// straight, not premultiplied. `0x33363AFF` is CSS `#33363A`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Fill {
	/// The colour.
	pub color: u32,
	/// The shape filled; the node's bounds when absent.
	pub shape: Option<Shape>,
}

/// A drop shadow under the node's fill, as CSS `box-shadow` draws it: the
/// node's shape, moved by `x` and `y`, blurred by `blur` (a Gaussian whose
/// standard deviation is half of `blur`), in `color`. Lengths are hundredths
/// of a logical pixel. The shadow takes no room in the layout.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct DropShadow {
	/// The shadow's colour.
	pub color: u32,
	/// Horizontal offset towards the end.
	pub x: i32,
	/// Vertical offset downwards.
	pub y: i32,
	/// Blur length.
	pub blur: u32,
}

/// Where a node is drawn, moved from where its layout put it, as CSS
/// `transform: translate()` moves it: its neighbours keep their places.
/// Hundredths of a logical pixel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Translate {
	/// Towards the end.
	pub x: i32,
	/// Downwards.
	pub y: i32,
}

/// The type of a `Text`, given by value instead of a [`TypographyStyle`]:
/// the host's text face (the design system's sans) at this size and weight.
/// Lengths are hundredths of a logical pixel. Each line stands `line_height`
/// tall with its glyphs centred in it, as a CSS line box does; a line too
/// long for its width ends in an ellipsis. Overrides the `Text`'s style and
/// colour; on any other node it does nothing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TextFont {
	/// The em size.
	pub size: u32,
	/// The height of each line.
	pub line_height: u32,
	/// CSS weight, 100 to 900.
	pub weight: u16,
	/// The text's colour.
	pub color: u32,
	/// Extra space after each character.
	pub letter_spacing: i32,
}

/// A node that fades as a keyed pan's children travel: its opacity is
/// multiplied by `1 - rate * progress`, clamped to 0 through 1, where progress
/// is how far the children of the `Gesture` keyed `key` are drawn along its
/// travel (0 at the start, 1 at the end), whoever moves them: the finger, a
/// worker's offsets, or the host settling them on release. A key that names no
/// gesture leaves the node as it is.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PanFade {
	/// The `key` of the gesture followed.
	pub key: String,
	/// Thousandths: 1400 is gone at 71% of the travel.
	pub rate_permille: u16,
}

/// A band of light sweeping across a `Text`'s glyphs and only them, as a CSS
/// gradient clipped to the text does. The band is `band` wide and runs from
/// `lead` before the text's start to `lead` past its end at `speed`, then
/// rests `rest_ms` before the next sweep. Its colour is `color` (alpha
/// ignored) with the alphas `stops` spread evenly across the band, laid over
/// the text's own colour. It starts when the node first appears, and the host
/// leaves it out where the person asked for less motion. Lengths are hundredths
/// of a logical pixel; `speed` is hundredths per second.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Shimmer {
	/// The band's colour.
	pub color: u32,
	/// Alphas, 0 to 255, from the band's start to its end.
	#[serde(deserialize_with = "hex_bytes_json::deserialize")]
	pub stops: Vec<u8>,
	/// The band's width.
	pub band: u32,
	/// How far before and after the text a sweep starts and ends.
	pub lead: u32,
	/// How fast the band moves.
	pub speed: u32,
	/// The pause after each sweep.
	pub rest_ms: u32,
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
	/// Background fill in a colour given by value.
	Fill(Fill),
	/// Drop shadow under the node's fill.
	Shadow(DropShadow),
	/// Draw the node moved, leaving its layout where it is.
	Translate(Translate),
	/// The type of a `Text`, given by value.
	Font(TextFont),
	/// Fade the node as a keyed pan's children travel.
	FadeWithPan(PanFade),
	/// A band of light sweeping a `Text`.
	Shimmer(Shimmer),
	/// Turn the node about its centre, once every this many milliseconds, for
	/// as long as it is shown; the host keeps it still where the person asked
	/// for less motion.
	Spin(u32),
	/// When a tree no longer holds this node, keep drawing it as it last was
	/// and fade it out over this many milliseconds, with CSS's `ease` curve
	/// (`cubic-bezier(0.25, 0.1, 0.25, 1)`), before it goes. It must keep its
	/// place among its siblings' indices for the host to know it left: the
	/// last child, or one whose followers leave with it.
	ExitFade(u16),
	/// Hides part of the node, or shows only part of it. A node keeps its last
	/// `Mask`.
	Mask(Mask),
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

/// Direction in which a pan recognizer accepts movement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PanAxis {
	/// Horizontal movement; vertical scrolling wins until horizontal intent.
	Horizontal,
	/// Vertical movement.
	Vertical,
	/// Movement in either direction.
	Free,
}

/// Distance available to a dragged child.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PanTravel {
	/// Parent width minus child width, clamped at zero.
	ParentWidth,
	/// Explicit travel in logical pixels.
	Points(Size),
}

/// How a host-owned drag settles when the finger lifts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PanRelease {
	/// Return to the starting position.
	SnapBack,
	/// Remain where the finger lifted.
	Stay,
	/// Settle to the end on success, otherwise return to the start.
	Complete {
		/// Percentage of travel needed, clamped by the host to 1 through 100.
		#[serde(rename = "thresholdPercent")]
		threshold_percent: u8,
	},
}

/// Motion owned by the host, retained while the node keeps its structural place.
///
/// Travel is clamped to a nonnegative range along the pan axis. Free pans move
/// in both axes, using the same travel limit for each.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PanDrag {
	/// Bounds used for movement and event fractions.
	pub travel: PanTravel,
	/// Settle behavior on release.
	pub release: PanRelease,
	/// Keep a completed drag at the end; absent means true.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub hold_at_end: OptionBool,
}

/// Recognizer configuration and optional actions for a pan.
///
/// With no drag, events describe clamped translation relative to the gesture's
/// start and fraction uses parent width minus child width. The worker may move
/// the child through new trees. A zero interval requests every move.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PanGesture {
	/// Accepted movement direction.
	pub axis: PanAxis,
	/// Present when the host moves the children itself.
	pub drag: Option<PanDrag>,
	/// Action on accepted pan start.
	pub start_action: Option<String>,
	/// Action on movement; absent sends no move events.
	pub move_action: Option<String>,
	/// Minimum milliseconds between move events; absent means zero.
	pub move_interval_ms: Option<u16>,
	/// Action on release or cancellation.
	pub end_action: Option<String>,
	/// Report where the finger is instead of how far it moved; absent means
	/// false.
	///
	/// Each event's `x` and `y` are then the finger's place inside the
	/// gesture, from its top-start corner and clamped to its frame, and
	/// `fraction_permille` is `x` as a share of the width. `Started` carries
	/// the touch-down point. The axis still decides which pans the gesture
	/// claims; there is no travel and no host-owned drag to clamp to. This is
	/// what a worker drawing under the finger needs.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub location: OptionBool,
}

/// A container tracking taps and pans over worker-drawn children.
///
/// Enabled defaults to true. Only declared actions are sent, each carrying a
/// SCALE-encoded [`GestureEvent`]. No visual styling is supplied by the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct GestureProps {
	/// Action on a tap that did not become a pan.
	pub tap_action: Option<String>,
	/// Optional pan recognizer.
	pub pan: Option<PanGesture>,
	/// Label for the accessibility action.
	pub accessibility_label: Option<String>,
	/// Whether gestures are accepted; absent means true.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub enabled: OptionBool,
	/// Success feedback for `Complete` releases; absent means false.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub haptic_on_complete: OptionBool,
	/// Names this gesture for [`RendererNode::SetOffset`] updates.
	///
	/// A keyed pan with a move action is paced by the host: at most one move
	/// per display frame, and none while the previous move is unanswered. A
	/// move is answered by the next item of the render stream: a `SetOffset`,
	/// a [`RendererNode::ExtendMask`] or a new tree. The host drops the moves
	/// in between and sends only the newest, so no backlog builds up. Absent
	/// keeps every move, as `moveIntervalMs` says.
	pub key: Option<String>,
	/// How the children move when pressed and when a pan lets go; absent
	/// leaves it to the host.
	pub feel: Option<PanFeel>,
}

/// How a gesture's children move when pressed and released, given by value so
/// a product's design keeps its own motion.
///
/// While a pan or a press holds, the children grow to `press_scale_permille`
/// (a quadratic ease-out over `press_ms`). When the finger lifts short of
/// completing, they go back to the start on a damped spring that starts at
/// rest (`spring_omega_centi`, `spring_damping_centi`) and back to their size
/// over `unpress_ms`. When it completes, they reach the end with a quadratic
/// ease-out lasting `approach_ms`, or `approach_ms * sqrt(distance /
/// approach_reference)` over a longer way, and play `pop_permille` from the
/// moment they would arrive: one scale per 1/60 s, the first replaced by the
/// scale they had, standing `pop_offset * ((scale - 1) / (peak - 1))^1.5`
/// past the end while larger than 1, where `peak` is the largest scale.
/// Lengths are hundredths of a logical pixel. The host draws none of this
/// where the person asked for less motion.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PanFeel {
	/// Scale while pressed, in thousandths.
	pub press_scale_permille: u16,
	/// How long the press takes to grow.
	pub press_ms: u16,
	/// How long a released child takes to return to its size.
	pub unpress_ms: u16,
	/// The return spring's angular frequency, hundredths of a radian a second.
	pub spring_omega_centi: u16,
	/// The return spring's damping ratio, in hundredths.
	pub spring_damping_centi: u16,
	/// The shortest way to the end on completion.
	pub approach_ms: u16,
	/// The distance beyond which the approach takes longer.
	pub approach_reference: u32,
	/// Scales after completion, one per 1/60 s, in thousandths.
	pub pop_permille: Vec<u16>,
	/// How far past the end the children stand at the pop's peak.
	pub pop_offset: u32,
}

/// Stage of a gesture action.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum GesturePhase {
	/// A tap with no pan.
	Tap,
	/// Accepted pan begins.
	Started,
	/// Finger moved.
	Moved,
	/// Finger lifted without a `Complete` release succeeding.
	Ended,
	/// Gesture recognizer cancelled; never completes an action.
	Cancelled,
	/// `Complete` threshold reached on release or by accessibility activation.
	Completed,
}

/// Payload of every gesture action, independent of the renderer context.
///
/// Coordinates are rounded logical-pixel offsets from the travel origin,
/// clamped to travel. Fractions range from 0 through 1000 along the primary
/// axis (horizontal for `Free`), and are zero when travel is zero. Taps use zero.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct GestureEvent {
	/// Recognizer lifecycle stage.
	pub phase: GesturePhase,
	/// Horizontal translation, in hundredths of a logical pixel: a whole
	/// pixel would let a slow drag report a new place only every few frames.
	pub x: i32,
	/// Vertical translation, in hundredths of a logical pixel.
	pub y: i32,
	/// Progress along the primary axis, in thousandths.
	pub fraction_permille: u16,
}

/// Whether a mask's shapes hide the node or are all of it that shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MaskMode {
	/// The node is hidden inside the shapes: they cut holes through it, and
	/// what is behind it shows through.
	Hide,
	/// The node shows only inside the shapes.
	Show,
}

/// A place in a masked node, in hundredths of a logical pixel from its
/// top-start corner, along the layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MaskPoint {
	/// Horizontal place, in hundredths of a logical pixel.
	pub x: i32,
	/// Vertical place, in hundredths of a logical pixel.
	pub y: i32,
}

/// One shape of a [`Mask`]. Places and sizes are in hundredths of a logical
/// pixel. A shape may reach outside the node; only the part inside counts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MaskShape {
	/// The whole node.
	All,
	/// A filled circle.
	Circle {
		/// Its centre.
		center: MaskPoint,
		/// Its radius.
		radius: u32,
	},
	/// A line through `points` in order, `width` wide, with round caps and
	/// joins. A single point is a dot `width` across.
	Stroke {
		/// The line's width.
		width: u32,
		/// The points it passes through.
		points: Vec<MaskPoint>,
	},
}

/// Lets the host draw a pan's own strokes into a [`Mask`] as the finger
/// moves, without waiting for the worker.
///
/// From the tree on, each stroke of the finger on the `Gesture` keyed `pan`
/// (touch-down to lift) is added to the mask as a [`MaskShape::Stroke`]
/// `width` wide through every place the host saw the finger, in the frame of
/// the touch: before the move is sent, and whether or not pacing drops it. The
/// pan must report its `location`. Its places are moved from the gesture's
/// frame to the masked node's by where the two are on screen. Like keyed
/// shapes, these strokes last until the next tree: a worker that wants them
/// kept puts them in that tree, as it heard them.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MaskFollow {
	/// The `key` of the `Gesture` whose location pan is drawn.
	pub pan: String,
	/// The stroke's width, in hundredths of a logical pixel.
	pub width: u32,
}

/// Hides part of a node, or shows only part of it, with shapes the worker
/// sets in the tree and can add to between trees.
///
/// Like `Opacity`, it applies to the node with its padding, background and
/// border, inside its margin. Each shape's edge is anti-aliased; there is no
/// feathering.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Mask {
	/// Names this mask for [`RendererNode::ExtendMask`] updates and for a fade
	/// between trees. Absent: the tree alone sets it.
	pub key: Option<String>,
	/// Whether the shapes hide the node or are all that shows of it.
	pub mode: MaskMode,
	/// The shapes, in drawing order. A place is inside the mask if any shape
	/// covers it.
	pub shapes: Vec<MaskShape>,
	/// Has the host draw a pan's strokes into the mask itself.
	pub follow: Option<MaskFollow>,
	/// When a new tree changes this keyed mask, the host cross-fades from what
	/// it drew to the tree's mask over this many milliseconds, on the CSS
	/// `ease` curve; absent or zero changes it at once. Keyed adds and followed
	/// strokes are always drawn at once. A tree arriving during a fade ends it
	/// at its target and fades on from there.
	pub fade_ms: Option<u16>,
	/// Has the host send an action once the shapes cover enough of the node:
	/// see [`ClearedShare`].
	pub cleared_share: Option<ClearedShare>,
}

/// Sends an action once a [`Mask`]'s shapes cover `at_permille` thousandths
/// of the node: for a `Hide` mask, the share cleared; for a `Show` mask, the
/// share shown. The host decides it, so every product and both hosts agree on
/// it, and a product only names the share and the action.
///
/// The shapes are the tree's, those added since by
/// [`RendererNode::ExtendMask`], and the followed pan's strokes. Their cover is
/// sampled at the centres of 2 by 2 logical pixel squares laid from the node's
/// top-start corner ([`cleared_share`]).
///
/// With [`Mask::follow`], the host checks when the finger lifts from that pan,
/// and again whenever shapes are added before it touches down again, so the
/// shapes a worker adds in answer to the pan's end count; nothing is sent
/// while the finger is down, and a new touch starts over. With no `follow`,
/// it checks whenever shapes are added. Either way it sends `action`, with no
/// payload, at most once per lift (with no `follow`, once per tree), and never
/// for the tree's own shapes alone: a tree that keeps the mask covered sends
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ClearedShare {
	/// The share of the node the shapes must cover, in thousandths. Zero is
	/// reached on any check; more than 1000 never.
	pub at_permille: u16,
	/// The action sent once the share is reached.
	pub action: String,
}

impl ClearedShare {
	/// Whether `covered` samples of `samples` reach the share.
	pub fn reached(&self, covered: u32, samples: u32) -> bool {
		u64::from(covered) * 1000 >= u64::from(self.at_permille) * u64::from(samples)
	}
}

/// The side of a [`cleared_share`] sample square, in hundredths of a logical
/// pixel.
pub const MASK_SAMPLE: u32 = 200;

/// How many of a node's samples its mask `shapes` cover, of how many, for a
/// node `width` by `height` hundredths of a logical pixel: the rule both hosts
/// follow for a [`ClearedShare`].
///
/// The node is laid with `ceil(width / 200)` by `ceil(height / 200)` samples,
/// each at the centre of its 2 by 2 logical pixel square from the top-start
/// corner (`(i * 200 + 100, j * 200 + 100)`). A sample is covered when it lies
/// in any shape: the whole node; a circle, at most `radius` from its centre; a
/// stroke, at most `width / 2` from its line (a single point is a dot).
pub fn cleared_share(shapes: &[MaskShape], width: u32, height: u32) -> (u32, u32) {
	let columns = width.div_ceil(MASK_SAMPLE);
	let rows = height.div_ceil(MASK_SAMPLE);
	let total = columns * rows;
	if shapes.iter().any(|shape| matches!(shape, MaskShape::All)) {
		return (total, total);
	}
	let mut covered = vec![false; total as usize];
	let mut mark =
		|left: f64, top: f64, right: f64, bottom: f64, inside: &dyn Fn(f64, f64) -> bool| {
			let first =
				|edge: f64| ((edge - 100.0) / f64::from(MASK_SAMPLE)).ceil().max(0.0) as u32;
			let last = |edge: f64, count: u32| {
				let at = ((edge - 100.0) / f64::from(MASK_SAMPLE)).floor();
				if at < 0.0 { None } else { Some((at as u32).min(count.saturating_sub(1))) }
			};
			let (Some(i1), Some(j1)) = (last(right, columns), last(bottom, rows)) else { return };
			for j in first(top)..=j1 {
				for i in first(left)..=i1 {
					let x = f64::from(i * MASK_SAMPLE + 100);
					let y = f64::from(j * MASK_SAMPLE + 100);
					if inside(x, y) {
						covered[(j * columns + i) as usize] = true;
					}
				}
			}
		};
	for shape in shapes {
		match shape {
			MaskShape::All => {},
			MaskShape::Circle { center, radius } => {
				let (cx, cy, r) = (f64::from(center.x), f64::from(center.y), f64::from(*radius));
				mark(cx - r, cy - r, cx + r, cy + r, &|x, y| {
					(x - cx).powi(2) + (y - cy).powi(2) <= r * r
				});
			},
			MaskShape::Stroke { width, points } => {
				let r = f64::from(*width) / 2.0;
				let at = |p: &MaskPoint| (f64::from(p.x), f64::from(p.y));
				let segments: Vec<((f64, f64), (f64, f64))> = match points.as_slice() {
					[] => Vec::new(),
					[one] => vec![(at(one), at(one))],
					many => many.windows(2).map(|pair| (at(&pair[0]), at(&pair[1]))).collect(),
				};
				for ((ax, ay), (bx, by)) in segments {
					let inside = |x: f64, y: f64| {
						let (dx, dy) = (bx - ax, by - ay);
						let length = dx * dx + dy * dy;
						let t = if length == 0.0 {
							0.0
						} else {
							(((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0)
						};
						(x - ax - t * dx).powi(2) + (y - ay - t * dy).powi(2) <= r * r
					};
					mark(ax.min(bx) - r, ay.min(by) - r, ax.max(bx) + r, ay.max(by) + r, &inside);
				}
			},
		}
	}
	(covered.iter().filter(|c| **c).count() as u32, total)
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

/// Properties of a `Materials` node.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MaterialsProps {
	/// Layers in drawing order.
	pub layers: Vec<MaterialInstance>,
}

/// One material applied with its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MaterialInstance {
	/// Material ID: the BLAKE2b-256 hash of its canonical text.
	#[serde(deserialize_with = "hex_bytes_json::deserialize")]
	pub material: [u8; 32],
	/// One argument per declared parameter, in declaration order.
	pub args: Vec<Parameter>,
}

/// One argument. Numbers are IEEE 754 single-precision bit patterns.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Parameter {
	/// A `float` parameter.
	Float(u32),
	/// A `vec2` parameter.
	Vec2([u32; 2]),
	/// A `vec3` parameter.
	Vec3([u32; 3]),
	/// A `vec4` parameter.
	Vec4([u32; 4]),
	/// A `samplerFloat` parameter, reading the image's first channel.
	SamplerFloat(ImageRef),
	/// A `samplerVec2` parameter, reading the image's first two channels.
	SamplerVec2(ImageRef),
	/// A `samplerVec3` parameter, reading the image's first three channels.
	SamplerVec3(ImageRef),
	/// A `samplerVec4` parameter, reading all four channels.
	SamplerVec4(ImageRef),
}

/// Where an image argument comes from.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ImageRef {
	/// A PNG, JPEG or WebP image, loaded like an `Image` node's source.
	Source(ImageSource),
	/// The host's noise image for this body, the same as `r_noise`.
	Noise,
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
	/// Draws its children, then runs its layers over them, as the materials
	/// RFC defines. As large as its children, which it places at the top start
	/// like a `Box`.
	#[codec(index = 11)]
	Materials {
		/// The layers to run.
		props: MaterialsProps,
		/// The content the layers draw over.
		children: Vec<RendererNode>,
	},
	/// Worker-drawn content with tap and pan interaction.
	#[codec(index = 12)]
	Gesture {
		/// Layout and styling.
		modifiers: Vec<Modifier>,
		/// Recognizers, actions and optional host-owned movement.
		props: GestureProps,
		/// Content to draw and optionally move.
		children: Vec<RendererNode>,
	},
	/// Moves the children of the `Gesture` whose `key` matches, without a new
	/// tree. Valid only as a whole item of a `render` stream; inside a tree it
	/// draws nothing.
	///
	/// The host adds `x` and `y` (hundredths of a logical pixel, along the
	/// layout direction) to where the current tree places those children, and
	/// keeps the offset until the next tree, which replaces it: a tree always
	/// wins. A key that
	/// matches no `Gesture` in the current tree is ignored. The item is not a
	/// tree: a Pocket card does not keep it as a face.
	#[codec(index = 13)]
	SetOffset {
		/// The `key` of the `Gesture` to move.
		key: String,
		/// Horizontal offset, in hundredths of a logical pixel.
		x: i32,
		/// Vertical offset, in hundredths of a logical pixel.
		y: i32,
	},
	/// Adds shapes to the [`Mask`] whose `key` matches, without a new tree.
	/// Valid only as a whole item of a `render` stream; inside a tree it draws
	/// nothing.
	///
	/// The host draws them at once, over the tree's own shapes, and keeps them
	/// until the next tree, which replaces them with its own: a tree always
	/// wins, so a worker that sends a tree puts in it every shape it wants
	/// kept. A key that matches no mask in the current tree is ignored. It
	/// answers a paced pan move, as a `SetOffset` does, with or without
	/// shapes. The item is not a tree: a Pocket card does not keep it as a
	/// face.
	#[codec(index = 14)]
	ExtendMask {
		/// The `key` of the `Mask` to add to.
		key: String,
		/// The shapes to add.
		shapes: Vec<MaskShape>,
	},
}

/// Where a product-rendered body lives, and the id that names it there.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RenderContext {
	/// A message in a chat room.
	ChatMessage {
		/// Room the message was posted in.
		room_id: String,
		/// Message id, as returned by `Chat::post_message`, or the
		/// `message_id` of a `ChatMessage` with a `Custom` body.
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

/// A `Materials` node that fell back.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRendererFailureSubscribeItem {
	/// Where the body lives.
	pub context: RenderContext,
	/// The node's position among the tree's `Materials` nodes, depth first,
	/// starting at 0.
	pub node: u32,
	/// The material that failed, when the failure belongs to one.
	pub material: Option<[u8; 32]>,
	/// Why the node fell back.
	pub reason: MaterialFailure,
}

/// Why a `Materials` node fell back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MaterialFailure {
	/// A material or image argument cannot be fetched, or has expired.
	NotFound,
	/// Fetched bytes do not match their ID.
	Integrity,
	/// The material breaks the format, with any format code but
	/// `unsupported-version` and `unsupported-feature`.
	InvalidMaterial,
	/// The arguments do not match the declared parameters, an image is not
	/// PNG, JPEG or WebP, or the noise image fills a parameter other than
	/// `samplerFloat`.
	InvalidStack,
	/// The node breaks a limit.
	LimitExceeded,
	/// The host does not read the version or a required feature, cannot
	/// compile the material, does not draw materials on this surface, or has
	/// materials switched off.
	Unsupported,
}

impl RendererNode {
	/// Empty the bytes of every inline image a host must not draw, so it
	/// draws as empty space at its size.
	///
	/// An inline image may be drawn only as PNG, JPEG or WebP, recognised by
	/// its first bytes, and at most 64 KiB. SVG is refused because it can load
	/// outside addresses.
	pub fn clear_refused_inline_images(&mut self) {
		match self {
			Self::Image { props, .. } => props.source.clear_if_refused(),
			Self::Materials { props, children } => {
				for argument in props.layers.iter_mut().flat_map(|layer| &mut layer.args) {
					if let Parameter::SamplerFloat(ImageRef::Source(source)) |
					Parameter::SamplerVec2(ImageRef::Source(source)) |
					Parameter::SamplerVec3(ImageRef::Source(source)) |
					Parameter::SamplerVec4(ImageRef::Source(source)) = argument
					{
						source.clear_if_refused();
					}
				}
				children.iter_mut().for_each(Self::clear_refused_inline_images);
			},
			Self::Box { children, .. } |
			Self::Column { children, .. } |
			Self::Row { children, .. } |
			Self::Text { children, .. } |
			Self::Gesture { children, .. } |
			Self::Button { children, .. } => children.iter_mut().for_each(Self::clear_refused_inline_images),
			Self::Nil |
			Self::String { .. } |
			Self::Spacer { .. } |
			Self::TextField { .. } |
			Self::SetOffset { .. } |
			Self::ExtendMask { .. } => {},
		}
	}
}

impl ImageSource {
	fn clear_if_refused(&mut self) {
		const MAX_INLINE_IMAGE_BYTES: usize = 64 * 1024;
		if let Self::Inline(bytes) = self {
			let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
			let jpeg = bytes.starts_with(&[0xff, 0xd8, 0xff]);
			let webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");
			if bytes.len() > MAX_INLINE_IMAGE_BYTES || !(png || jpeg || webp) {
				bytes.clear();
			}
		}
	}
}

#[cfg(test)]
mod tests;
