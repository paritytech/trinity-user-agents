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

/// An easing curve, as CSS `cubic-bezier(x1, y1, x2, y2)` draws it, with its
/// control points in thousandths: `{ x1: 250, y1: 100, x2: 250, y2: 1000 }`
/// is CSS's `ease`, and `{ 0, 0, 1000, 1000 }` is linear.
///
/// The host clamps `x1` and `x2` to 0 through 1000; `y1` and `y2` may leave
/// that range to overshoot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Curve {
	/// The first control point's time.
	pub x1: i16,
	/// The first control point's progress.
	pub y1: i16,
	/// The second control point's time.
	pub x2: i16,
	/// The second control point's progress.
	pub y2: i16,
}

impl Curve {
	/// The progress at `time`, both shares of the whole (`time` clamped to 0
	/// through 1): the rule both hosts follow.
	pub fn ease(&self, time: f64) -> f64 {
		let time = time.clamp(0.0, 1.0);
		let thousandths = |value: i16| f64::from(value) / 1000.0;
		let (x1, x2) = (thousandths(self.x1).clamp(0.0, 1.0), thousandths(self.x2).clamp(0.0, 1.0));
		let (y1, y2) = (thousandths(self.y1), thousandths(self.y2));
		let bezier = |first: f64, second: f64, at: f64| {
			let rest = 1.0 - at;
			3.0 * rest * rest * at * first + 3.0 * rest * at * at * second + at * at * at
		};
		let (mut low, mut high) = (0.0, 1.0);
		for _ in 0..50 {
			let middle = (low + high) / 2.0;
			if bezier(x1, x2, middle) < time {
				low = middle;
			} else {
				high = middle;
			}
		}
		bezier(y1, y2, (low + high) / 2.0)
	}
}

/// A damped spring that starts at rest, in the units [`PanFeel`]'s return
/// spring uses.
///
/// It ends once it is within a thousandth of the change from its target and
/// moves less than that in 1/60 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Spring {
	/// Angular frequency, in hundredths of a radian a second.
	pub omega_centi: u16,
	/// Damping ratio, in hundredths: 100 is critically damped, less bounces.
	pub damping_centi: u16,
}

/// How the host moves a value to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Motion {
	/// Over a set time, along a curve.
	Timed {
		/// How long it takes, in milliseconds.
		ms: u32,
		/// The curve it follows.
		curve: Curve,
	},
	/// On a spring, for as long as it takes to settle.
	Spring(Spring),
}

/// Where a node enters from, relative to how the tree draws it: its opacity
/// and scale multiplied, its place and turn added, about its centre and after
/// layout, as [`Translate`] moves it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Pose {
	/// Opacity, in thousandths of the node's own; clamped to 0 through 1000.
	pub opacity_permille: u16,
	/// Scale, in thousandths.
	pub scale_permille: u16,
	/// Moved towards the end, in hundredths of a logical pixel.
	pub x: i32,
	/// Moved downwards, in hundredths of a logical pixel.
	pub y: i32,
	/// Turned clockwise, in hundredths of a degree.
	pub rotation_centideg: i32,
}

/// Has the host play a node in from a [`Pose`] when it appears: when a tree
/// holds it at a place among its siblings that the tree before did not, and
/// for every node of the first tree.
///
/// The node holds `from` for `delay_ms`, then moves to how the tree draws it
/// with `motion`. A tree arriving while it plays keeps it playing. It plays
/// once per appearance, and not at all where the person asked for less
/// motion; a host that cannot play it draws the node as the tree does.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Enter {
	/// Where it starts.
	pub from: Pose,
	/// How it moves.
	pub motion: Motion,
	/// How long it waits first, in milliseconds.
	pub delay_ms: u16,
}

/// A part of how a node is drawn that a [`Track`] moves, relative to how the
/// tree draws it and after layout, about the node's centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum AnimatedProperty {
	/// Opacity, in thousandths of the node's own; clamped to 0 through 1000.
	Opacity,
	/// Scale, in thousandths.
	Scale,
	/// Moved towards the end, in hundredths of a logical pixel.
	OffsetX,
	/// Moved downwards, in hundredths of a logical pixel.
	OffsetY,
	/// Turned clockwise, in hundredths of a degree.
	Rotation,
}

/// One value of a [`Keyframes`] timeline.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Keyframe {
	/// When it is reached, in milliseconds from the start of a run.
	pub at_ms: u32,
	/// The value, in the property's units.
	pub value: i32,
	/// The curve from the keyframe before to this one; the first keyframe's
	/// is not used.
	pub curve: Curve,
}

/// A timeline of values the host plays on its own clock from when the node
/// appears, as a CSS `@keyframes` animation does.
///
/// A run lasts until the last keyframe; before the first, the value is the
/// first's. Keyframes are in order of `at_ms`: one earlier than the keyframe
/// before it is taken at that one's time, and at a time several share, the
/// last of them holds, so two keyframes at one time make a jump. `repeat` is how many runs play,
/// zero for ever; with `alternate`, every second run plays backwards. After
/// the last run the value holds where that run ended. A tree that keeps the
/// same timeline keeps its clock; a tree that changes it starts it over. The
/// host draws at most 32 keyframes, holds the end where the person asked for
/// less motion (the first keyframe for a timeline that plays for ever), and a
/// host that cannot play it draws that value.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Keyframes {
	/// The values, in order.
	pub frames: Vec<Keyframe>,
	/// How many runs play; zero plays for ever.
	pub repeat: u16,
	/// Play every second run backwards; absent means false.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	pub alternate: OptionBool,
}

impl Keyframes {
	/// The value `elapsed_ms` after the timeline started, or `None` with no
	/// keyframes: the rule both hosts follow.
	pub fn value_at(&self, elapsed_ms: u64) -> Option<f64> {
		let mut frames = Vec::with_capacity(self.frames.len());
		let mut latest = 0;
		for frame in &self.frames {
			latest = latest.max(frame.at_ms);
			frames.push((latest, f64::from(frame.value), frame.curve));
		}
		let (&(first_ms, first_value, _), &(run_ms, last_value, _)) =
			(frames.first()?, frames.last()?);
		if run_ms == 0 {
			return Some(last_value);
		}
		let runs = elapsed_ms / u64::from(run_ms);
		let (run, within) = if self.repeat != 0 && runs >= u64::from(self.repeat) {
			(u64::from(self.repeat) - 1, run_ms)
		} else {
			(runs, u32::try_from(elapsed_ms % u64::from(run_ms)).unwrap_or(run_ms))
		};
		let backwards = self.alternate.0 == Some(true) && run % 2 == 1;
		let at = if backwards { run_ms - within } else { within };
		if let Some(&(_, value, _)) = frames.iter().rev().find(|(frame_ms, ..)| *frame_ms == at) {
			return Some(value);
		}
		if at < first_ms {
			return Some(first_value);
		}
		let next = frames.iter().position(|(frame_ms, ..)| *frame_ms > at)?;
		let ((from_ms, from_value, _), (to_ms, to_value, curve)) = (frames[next - 1], frames[next]);
		let time = f64::from(at - from_ms) / f64::from(to_ms - from_ms);
		Some(from_value + (to_value - from_value) * curve.ease(time))
	}
}

/// How a [`Track`] sets its property.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum TrackPlay {
	/// A timeline the host plays on its own.
	Keyframes(Keyframes),
	/// The value `target`, which the node first appears at; when a later tree
	/// changes it, the host moves there from the value it draws with `motion`,
	/// a spring keeping its speed. Where the person asked for less motion, or
	/// on a host that cannot move it, the value changes at once.
	Follow {
		/// The value, in the property's units.
		target: i32,
		/// How a change moves.
		motion: Motion,
	},
}

/// Moves one part of how a node is drawn on the host's clock, with no update
/// from the worker per frame: opacity and scale multiply the node's own,
/// offsets and turn add to them, about its centre and after layout. A node
/// keeps its last track for each property, and its tracks compose with its
/// `Enter`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Track {
	/// The part moved.
	pub property: AnimatedProperty,
	/// How it moves.
	pub play: TrackPlay,
}

/// What one particle of an [`Emitter`] looks like. Sizes are hundredths of a
/// logical pixel and colours `0xRRGGBBAA`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ParticleLook {
	/// A filled circle.
	Circle {
		/// Its diameter.
		diameter: u32,
		/// Its colour.
		color: u32,
	},
	/// A filled rectangle.
	Rectangle {
		/// Its width.
		width: u32,
		/// Its height.
		height: u32,
		/// Its colour.
		color: u32,
	},
	/// An image drawn at this size, fetched or refused as an `Image` node's
	/// is; one that cannot be drawn draws no particle.
	Image {
		/// Where its bytes come from.
		source: ImageSource,
		/// Its width.
		width: u32,
		/// Its height.
		height: u32,
	},
}

/// Where an [`Emitter`]'s particles start, in the node's frame.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum EmitSource {
	/// Where the finger is on the `Gesture` with this `key`, whose pan must
	/// report its `location`; its places are moved to the node's frame by
	/// where the two are on screen, as a [`MaskFollow`]'s are. It emits only
	/// while the finger is down, along the way it went between frames, and
	/// bursts from where the finger last was.
	Pan(String),
	/// A place in the node, in hundredths of a logical pixel from its
	/// top-start corner.
	Point(MaskPoint),
	/// Anywhere in the node, evenly at random.
	Area,
}

/// A value an [`Emitter`] picks for each particle, evenly at random between
/// `min` and `max` (the two swapped when `max` is less).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ValueRange {
	/// The least value.
	pub min: i32,
	/// The greatest value.
	pub max: i32,
}

impl ValueRange {
	/// The value at `share` of the way from `min` to `max`, `share` being the
	/// host's even random number in 0 through 1.
	pub fn pick(&self, share: f64) -> f64 {
		let (low, high) = (f64::from(self.min.min(self.max)), f64::from(self.min.max(self.max)));
		low + (high - low) * share.clamp(0.0, 1.0)
	}
}

/// What sets off an emitter's [`Burst`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(tag = "tag", content = "value")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum BurstTrigger {
	/// The node appearing, as an [`Enter`] plays.
	Appear,
	/// The host sending this action from the body's current tree, whichever
	/// node sends it: a button, a gesture's phase, a mask's cleared share. The
	/// burst starts as the action is sent, with no round trip to the worker.
	Action(String),
	/// A tree carrying a different value here than the tree before it, which
	/// lets a worker set off a burst by changing a count. The first tree with
	/// the emitter never bursts.
	Change(u32),
}

/// A number of particles an [`Emitter`] sends at once.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Burst {
	/// How many particles.
	pub count: u16,
	/// What sets it off.
	pub on: BurstTrigger,
}

/// Sends particles from a node, drawn and moved by the host on its own
/// clock: confetti, sparks, dust or debris.
///
/// Each particle takes one of `looks` at random and starts at the `source`.
/// It leaves at a speed picked from `speed`, in a direction `angle_centideg`
/// clockwise from the end direction, spread evenly across `spread_centideg`
/// around it, and falls with `gravity`. It turns at a speed picked from
/// `spin`, from upright. It is drawn whole until `fade_from_permille` of its
/// life, then fades to nothing at its end, and its size goes from its look's
/// to `end_scale_permille` of it over its life. [`Self::place_at`] is the
/// rule both hosts follow.
///
/// Particles are drawn over the node's content and its children, in its
/// frame and unclipped, and take no room in the layout. They keep moving
/// across trees that keep the emitter; a tree without it lets the live ones
/// finish. At most `max_alive` are alive at once (the host caps it at 512);
/// the rest are not sent. The host sends none where the person asked for less
/// motion, and a host that cannot draw them draws the node alone.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Emitter {
	/// What the particles look like; empty sends none.
	pub looks: Vec<ParticleLook>,
	/// Where they start.
	pub source: EmitSource,
	/// Particles sent each second while the source is live: while the
	/// finger is down for a pan, always for a point or the area.
	pub rate_per_s: u16,
	/// Particles sent for each 100 logical pixels the finger moves, for a pan
	/// source; spaced evenly along the way.
	pub rate_per_100pt: u16,
	/// How long each lives, in milliseconds.
	pub life_ms: u16,
	/// Speed, in hundredths of a logical pixel a second.
	pub speed: ValueRange,
	/// The middle direction, in hundredths of a degree clockwise from the end
	/// direction: 9000 is down, -9000 up.
	pub angle_centideg: i16,
	/// The full width of the directions around it, in hundredths of a degree.
	pub spread_centideg: u16,
	/// Downward acceleration, in hundredths of a logical pixel a second
	/// squared; negative rises.
	pub gravity: i32,
	/// Turning speed, in hundredths of a degree a second, clockwise.
	pub spin: ValueRange,
	/// The share of life after which a particle fades, in thousandths; 1000
	/// or more never fades.
	pub fade_from_permille: u16,
	/// The size at the end of life, in thousandths of the look's.
	pub end_scale_permille: u16,
	/// The most particles alive at once.
	pub max_alive: u16,
	/// Particles sent at once when something happens.
	pub bursts: Vec<Burst>,
}

impl Emitter {
	/// Where a particle is `age_ms` after it left `start` at `speed` in
	/// `direction` (in radians, clockwise from the end direction), with its
	/// opacity and scale as shares of its look's: `(x, y, opacity, scale)`,
	/// or `None` once its life is over. Places and speeds are hundredths of a
	/// logical pixel, and per second.
	pub fn place_at(
		&self,
		start: (f64, f64),
		speed: f64,
		direction: f64,
		age_ms: u32,
	) -> Option<(f64, f64, f64, f64)> {
		if age_ms >= u32::from(self.life_ms) {
			return None;
		}
		let seconds = f64::from(age_ms) / 1000.0;
		let life = f64::from(age_ms) / f64::from(self.life_ms);
		let x = start.0 + speed * direction.cos() * seconds;
		let y = start.1 +
			speed * direction.sin() * seconds +
			f64::from(self.gravity) * seconds * seconds / 2.0;
		let fade_from = (f64::from(self.fade_from_permille) / 1000.0).min(1.0);
		let opacity = if life <= fade_from { 1.0 } else { (1.0 - life) / (1.0 - fade_from) };
		let end_scale = f64::from(self.end_scale_permille) / 1000.0;
		Some((x, y, opacity, 1.0 + (end_scale - 1.0) * life))
	}
}

/// How a [`Parallax`] follows the device's tilt.
///
/// The tilt is measured from a rest attitude: the device's when the node
/// appeared, which then follows the device's own with a time constant of
/// `settle_ms` (zero keeps it), so the node drifts home when the device is
/// held still another way. Each axis's tilt, clamped to `max_tilt_centideg`
/// either way, moves the node in proportion, up to `travel` at the clamp:
/// lowering the device's end edge (its right, in a left-to-right layout)
/// moves it towards the end, and lowering its top edge moves it down.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TiltParallax {
	/// The move at the clamp, in hundredths of a logical pixel.
	pub travel: u32,
	/// The tilt that moves it all the way, in hundredths of a degree; zero
	/// never moves.
	pub max_tilt_centideg: u16,
	/// How fast the rest attitude follows the device, in milliseconds.
	pub settle_ms: u16,
}

/// Moves a node by the device's tilt, a keyed pan's children, or both,
/// scaled by its depth, so layers at different depths move apart.
///
/// The move is `depth_permille / 1000` times the sum of the tilt's (see
/// [`TiltParallax`]) and the offset at which the children of the `Gesture`
/// keyed `pan` are drawn, whoever moves them: the finger, a worker's offsets,
/// or the host settling them. 1000 moves with them, 0 not at all, a negative
/// depth the other way. It is drawn after layout, as [`Translate`] moves the
/// node, and composes with it and with tracks. [`Self::offset`] is the rule
/// both hosts follow. A key that names no gesture adds nothing. Where the
/// person asked for less motion, or on a host with no tilt to read, the tilt
/// adds nothing; the pan's part still follows the finger.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Parallax {
	/// How far the node moves for its sources, in thousandths.
	pub depth_permille: i16,
	/// Follows the device's tilt; absent does not.
	pub tilt: Option<TiltParallax>,
	/// The `key` of the gesture whose children's offset it follows; absent
	/// does not.
	pub pan: Option<String>,
}

impl Parallax {
	/// The node's move, in hundredths of a logical pixel along the layout
	/// direction and down, for a `tilt` from the rest attitude in degrees
	/// (end edge lowered, top edge lowered) and the pan's children drawn at
	/// `pan_offset`.
	pub fn offset(&self, tilt: (f64, f64), pan_offset: (f64, f64)) -> (f64, f64) {
		let (tilt_x, tilt_y) = match &self.tilt {
			Some(follow) if follow.max_tilt_centideg > 0 => {
				let max = f64::from(follow.max_tilt_centideg) / 100.0;
				let travel = f64::from(follow.travel);
				let along = |degrees: f64| travel * (degrees / max).clamp(-1.0, 1.0);
				(along(tilt.0), along(tilt.1))
			},
			_ => (0.0, 0.0),
		};
		let (pan_x, pan_y) = if self.pan.is_some() { pan_offset } else { (0.0, 0.0) };
		let depth = f64::from(self.depth_permille) / 1000.0;
		(depth * (tilt_x + pan_x), depth * (tilt_y + pan_y))
	}
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
	/// Plays the node in when it appears.
	Enter(Enter),
	/// Moves a part of how the node is drawn on the host's clock.
	Animate(Track),
	/// Sends particles from the node. A node keeps its last `Emit`.
	Emit(Emitter),
	/// Moves the node by the device's tilt or a pan, scaled by its depth. A
	/// node keeps its last `Parallax`.
	Parallax(Parallax),
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
/// across and `press_scale_y_permille` (or the same) down, about their centre
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
	/// Scale while pressed, in thousandths: across, and down too unless
	/// `press_scale_y_permille` is set.
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
	/// Scale while pressed down the y axis, in thousandths, so a press can
	/// squash or stretch: `press_scale_permille` is then the x scale alone.
	/// Absent scales both axes alike.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub press_scale_y_permille: Option<u16>,
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
	/// A line through `points` in order drawn with a shaped tip: copies of
	/// `brush`, `width` long, stamped along it ([`MaskBrush`]). A single point
	/// is one stamp.
	Stamped {
		/// The tip's length along its own axis.
		width: u32,
		/// The tip and how far apart its stamps are.
		brush: MaskBrush,
		/// The points it passes through.
		points: Vec<MaskPoint>,
	},
}

/// The outline of a [`MaskBrush`] tip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MaskBrushTip {
	/// An ellipse: the stroke's width along its axis, the aspect's share of it
	/// across.
	Ellipse,
	/// A rectangle with fully round ends, as long and as thick as the ellipse.
	Capsule,
}

/// A shaped tip for a mask stroke, such as a slanted nib: copies of the tip
/// stamped along the line instead of a round pen.
///
/// The tip is the stroke's `width` long along its own axis and
/// `width * aspect_permille / 1000` across, turned `angle_centideg` from the
/// horizontal (clockwise, as the y axis points down). The stamps keep that
/// angle whichever way the line goes. The first stamp is centred on the first
/// point, and each next one `width * spacing_permille / 1000` further along
/// the line, the distance carried across its corners, with one more on the
/// last point when the spacing leaves it bare. The host clamps
/// `aspect_permille` to 1 through 1000 and `spacing_permille` to 50 through
/// 4000. Stamps are never closer than [`MASK_STAMP_SPACING_MIN`], and a line
/// is never stamped more than [`MASK_STAMPS_MAX`] times: a line longer than
/// that many spacings has its spacing stretched to its length over
/// `MASK_STAMPS_MAX - 1`, and no stamp past the limit is placed. A host that cannot stamp draws the
/// line as a round stroke of the same width, and counts it so.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MaskBrush {
	/// The tip's outline.
	pub tip: MaskBrushTip,
	/// The tip's thickness as a share of its length, in thousandths.
	pub aspect_permille: u16,
	/// The tip's turn from the horizontal, in hundredths of a degree.
	pub angle_centideg: i16,
	/// The distance between stamps as a share of the width, in thousandths.
	pub spacing_permille: u16,
}

/// The least distance between two stamps of a [`MaskShape::Stamped`] line,
/// in hundredths of a logical pixel: half a logical pixel.
pub const MASK_STAMP_SPACING_MIN: u32 = 50;

/// The most stamps a [`MaskShape::Stamped`] line has.
pub const MASK_STAMPS_MAX: usize = 4096;

impl MaskBrush {
	/// The centres of the stamps along `points` for a tip `width` long, in
	/// hundredths of a logical pixel: the rule both hosts follow.
	pub fn stamps(&self, width: u32, points: &[MaskPoint]) -> Vec<(f64, f64)> {
		let at = |point: &MaskPoint| (f64::from(point.x), f64::from(point.y));
		let (Some(first), Some(last)) = (points.first(), points.last()) else { return Vec::new() };
		let length_of = |pair: &[MaskPoint]| {
			let ((ax, ay), (bx, by)) = (at(&pair[0]), at(&pair[1]));
			(bx - ax).hypot(by - ay)
		};
		let total: f64 = points.windows(2).map(length_of).sum();
		let spacing = (f64::from(width) * f64::from(self.spacing_permille.clamp(50, 4000)) /
			1000.0)
			.max(f64::from(MASK_STAMP_SPACING_MIN))
			.max(total / (MASK_STAMPS_MAX - 1) as f64);
		let mut centres = vec![at(first)];
		let mut since_last = 0.0;
		for pair in points.windows(2) {
			let ((ax, ay), (bx, by)) = (at(&pair[0]), at(&pair[1]));
			let length = length_of(pair);
			let mut along = spacing - since_last;
			while along <= length && centres.len() < MASK_STAMPS_MAX {
				let t = along / length;
				centres.push((ax + t * (bx - ax), ay + t * (by - ay)));
				along += spacing;
			}
			since_last = length - (along - spacing);
		}
		if since_last > 0.0 && centres.len() < MASK_STAMPS_MAX {
			centres.push(at(last));
		}
		centres
	}

	/// Whether `(x, y)` lies in the stamp centred at `centre` for a tip
	/// `width` long.
	fn covers(&self, width: u32, centre: (f64, f64), x: f64, y: f64) -> bool {
		let half_length = f64::from(width) / 2.0;
		let half_thickness = half_length * f64::from(self.aspect_permille.clamp(1, 1000)) / 1000.0;
		let angle = (f64::from(self.angle_centideg) / 100.0).to_radians();
		let (dx, dy) = (x - centre.0, y - centre.1);
		let along = dx * angle.cos() + dy * angle.sin();
		let across = -dx * angle.sin() + dy * angle.cos();
		match self.tip {
			MaskBrushTip::Ellipse => {
				(along / half_length).powi(2) + (across / half_thickness).powi(2) <= 1.0
			},
			MaskBrushTip::Capsule => {
				let core = (half_length - half_thickness).max(0.0);
				let beyond = (along.abs() - core).max(0.0);
				beyond * beyond + across * across <= half_thickness * half_thickness
			},
		}
	}
}

/// Lets the host draw a pan's own strokes into a [`Mask`] as the finger
/// moves, without waiting for the worker.
///
/// From the tree on, each stroke of the finger on the `Gesture` keyed `pan`
/// (touch-down to lift) is added to the mask as a [`MaskShape::Stroke`]
/// `width` wide, or a [`MaskShape::Stamped`] with a `brush`, through every place the host saw the
/// finger, in the frame of the touch: before the move is sent, and whether or not pacing drops it.
/// The pan must report its `location`. Its places are moved from the gesture's
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
	/// The tip the strokes are drawn with: each is then added as a
	/// [`MaskShape::Stamped`]. Absent draws them round.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub brush: Option<MaskBrush>,
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
	/// Has the host play haptics as the shapes grow: see [`MaskHaptics`].
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub haptics: Option<MaskHaptics>,
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
/// shapes a worker adds in answer to the pan's end count; with `while_down`
/// it also checks while the finger is down, at most once per display frame.
/// A new touch starts over. With no `follow`, it checks whenever shapes are
/// added. Either way it sends `action`, with no payload, at most once per lift
/// (with no `follow`, once per tree), and never for the tree's own shapes
/// alone: a tree that keeps the mask covered sends nothing. A [`ShareRescue`]
/// sends it too when the shapes stop growing short of the share.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ClearedShare {
	/// The share of the node the shapes must cover, in thousandths. Zero is
	/// reached on any check; more than 1000 never.
	pub at_permille: u16,
	/// The action sent once the share is reached.
	pub action: String,
	/// Also check while the followed finger is down, so the action is sent
	/// mid-stroke instead of on the lift; absent means false. Without a
	/// `follow` it changes nothing.
	#[serde(
		default = "option_bool_json::absent",
		deserialize_with = "option_bool_json::deserialize"
	)]
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub while_down: OptionBool,
	/// Sends the action when the shapes stop growing short of the share.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub rescue: Option<ShareRescue>,
}

/// Sends a [`ClearedShare`]'s action when the shapes have covered at least
/// `from_permille` of the node but stopped growing short of its share, so a
/// last corner nobody can reach does not hold the node back.
///
/// The host keeps the time the cover last grew: the tree's arrival, then each
/// added or followed shape that covers a new sample. Once `after_ms` pass with
/// no growth while the cover is at least `from_permille`, whether the finger
/// is down or not, it sends the action as if the share were reached, at most
/// once per tree. A new tree starts the time over. A `from_permille` above
/// the share's own never sends.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ShareRescue {
	/// How long the cover must stand still, in milliseconds.
	pub after_ms: u32,
	/// The least cover that may be rescued, in thousandths.
	pub from_permille: u16,
}

/// A feedback the host plays through the device's haptics, from its own
/// set: the lightest is a selection tick, as a picker plays between values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Haptic {
	/// A selection tick.
	Tick,
	/// A light impact.
	Light,
	/// A medium impact.
	Medium,
	/// A heavy impact.
	Heavy,
	/// A success notice.
	Success,
}

/// Haptics a [`Mask`] plays as its shapes grow: `step` each time the cover
/// passes another `every_permille` of the node, and `reach` when its
/// [`ClearedShare`] is reached (by its share or a rescue).
///
/// The cover is the share [`cleared_share`] counts. The host measures it as
/// added and followed shapes arrive, at most once per display frame, and
/// plays `step` once for a frame however many steps it passed. Steps count
/// from the cover the tree gave, so a tree plays nothing, and a mask with no
/// shapes added between trees never plays. `every_permille` zero plays no
/// steps. The host plays nothing where the person turned haptics off, and a
/// host with no haptics ignores them.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct MaskHaptics {
	/// The cover between steps, in thousandths.
	pub every_permille: u16,
	/// The feedback for a step.
	pub step: Haptic,
	/// The feedback when the share is reached; absent plays none.
	pub reach: Option<Haptic>,
}

impl MaskHaptics {
	/// How many steps lie between a cover of `from` and one of `to`, both in
	/// thousandths: the host plays `step` once when this is more than zero.
	pub fn steps_passed(&self, from: u16, to: u16) -> u16 {
		if self.every_permille == 0 || to <= from {
			return 0;
		}
		to / self.every_permille - from / self.every_permille
	}
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
/// stroke, at most `width / 2` from its line (a single point is a dot); a
/// stamped line, inside or on the edge of any of its
/// [`MaskBrush::stamps`].
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
			MaskShape::Stamped { width, brush, points } => {
				let reach = f64::from(*width) / 2.0;
				for centre in brush.stamps(*width, points) {
					mark(
						centre.0 - reach,
						centre.1 - reach,
						centre.0 + reach,
						centre.1 + reach,
						&|x, y| brush.covers(*width, centre, x, y),
					);
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
	/// draws as empty space at its size: `Image` nodes, material arguments and
	/// particle looks.
	///
	/// An inline image may be drawn only as PNG, JPEG or WebP, recognised by
	/// its first bytes, and at most 64 KiB. SVG is refused because it can load
	/// outside addresses.
	pub fn clear_refused_inline_images(&mut self) {
		for modifier in self.modifiers_mut() {
			if let Modifier::Emit(emitter) = modifier {
				for look in &mut emitter.looks {
					if let ParticleLook::Image { source, .. } = look {
						source.clear_if_refused();
					}
				}
			}
		}
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

impl RendererNode {
	fn modifiers_mut(&mut self) -> &mut [Modifier] {
		match self {
			Self::Box { modifiers, .. } |
			Self::Column { modifiers, .. } |
			Self::Row { modifiers, .. } |
			Self::Spacer { modifiers } |
			Self::Text { modifiers, .. } |
			Self::Button { modifiers, .. } |
			Self::TextField { modifiers, .. } |
			Self::Image { modifiers, .. } |
			Self::Gesture { modifiers, .. } => modifiers,
			Self::Nil |
			Self::String { .. } |
			Self::Materials { .. } |
			Self::SetOffset { .. } |
			Self::ExtendMask { .. } => &mut [],
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
