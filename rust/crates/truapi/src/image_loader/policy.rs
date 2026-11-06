//! What the core passes on as an image, judged from the file header without decoding pixels.

use imagesize::ImageType;

use super::ImageFormat;

/// Largest image the core passes on.
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
/// Longest side in pixels. Decoders size a frame's buffer from the header, so this bounds the
/// memory one frame takes. The number of frames in an animation is not bounded.
const MAX_IMAGE_SIDE: usize = 4096;

/// Why bytes are not an image the core passes on.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Rejection {
	/// Over [`MAX_IMAGE_BYTES`].
	#[display("{bytes} bytes is over the {MAX_IMAGE_BYTES}-byte limit")]
	TooLarge {
		/// Size of the file.
		bytes: usize,
	},
	/// Not PNG, JPEG, GIF or WebP, which includes every SVG.
	#[display("not a PNG, JPEG, GIF or WebP image")]
	UnsupportedFormat,
	/// The bytes are in a format other than the one their source declares.
	#[display("declared {declared:?}, but the bytes are {actual:?}")]
	FormatMismatch {
		/// The source's format.
		declared: ImageFormat,
		/// The format read from the bytes.
		actual: ImageFormat,
	},
	/// The header ends before it gives the image's size.
	#[display("the header does not give the image size")]
	UnreadableSize,
	/// A side is over [`MAX_IMAGE_SIDE`].
	#[display("{width}x{height} pixels is over the {MAX_IMAGE_SIDE}-pixel side limit")]
	TooManyPixels {
		/// Width in pixels.
		width: usize,
		/// Height in pixels.
		height: usize,
	},
}

/// The format of `bytes`, once they pass the policy. `declared` is the format the image's source
/// names, if it names one.
pub fn check(bytes: &[u8], declared: Option<ImageFormat>) -> Result<ImageFormat, Rejection> {
	if bytes.len() > MAX_IMAGE_BYTES {
		return Err(Rejection::TooLarge { bytes: bytes.len() });
	}
	let format = match imagesize::image_type(bytes) {
		Ok(ImageType::Png) => ImageFormat::Png,
		Ok(ImageType::Jpeg) => ImageFormat::Jpeg,
		Ok(ImageType::Gif) => ImageFormat::Gif,
		Ok(ImageType::Webp) => ImageFormat::Webp,
		_ => return Err(Rejection::UnsupportedFormat),
	};
	if let Some(declared) = declared.filter(|declared| *declared != format) {
		return Err(Rejection::FormatMismatch { declared, actual: format });
	}
	let size = imagesize::blob_size(bytes).map_err(|_| Rejection::UnreadableSize)?;
	if size.width > MAX_IMAGE_SIDE || size.height > MAX_IMAGE_SIDE {
		return Err(Rejection::TooManyPixels { width: size.width, height: size.height });
	}
	Ok(format)
}

#[cfg(test)]
mod tests;
