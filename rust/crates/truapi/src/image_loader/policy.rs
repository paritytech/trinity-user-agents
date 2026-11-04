//! What the core passes on as an image, judged from the file header without decoding pixels.

use imagesize::ImageType;

use super::ImageFormat;

/// Largest image the core passes on.
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
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
mod tests {
	use super::*;

	fn png(width: u32, height: u32) -> Vec<u8> {
		[b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".as_slice(), &width.to_be_bytes(), &height.to_be_bytes()]
			.concat()
	}

	/// Only the header is read, so a truncated or hostile file is judged before any decoder
	/// allocates for it.
	#[test]
	fn images_are_judged_by_their_header() {
		let gif =
			[b"GIF89a".as_slice(), &16u16.to_le_bytes(), &16u16.to_le_bytes(), &[0, 0, 0]].concat();
		let mut oversized = png(16, 16);
		oversized.resize(MAX_IMAGE_BYTES + 1, 0);

		assert_eq!(
			[
				check(&png(4096, 16), None),
				check(&gif, Some(ImageFormat::Gif)),
				check(&png(16, 4097), None),
				check(&gif, Some(ImageFormat::Png)),
				check(br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#, None),
				check(&png(16, 16)[..20], None),
				check(&oversized, None),
			],
			[
				Ok(ImageFormat::Png),
				Ok(ImageFormat::Gif),
				Err(Rejection::TooManyPixels { width: 16, height: 4097 }),
				Err(Rejection::FormatMismatch {
					declared: ImageFormat::Png,
					actual: ImageFormat::Gif
				}),
				Err(Rejection::UnsupportedFormat),
				Err(Rejection::UnreadableSize),
				Err(Rejection::TooLarge { bytes: MAX_IMAGE_BYTES + 1 }),
			]
		);
	}
}
