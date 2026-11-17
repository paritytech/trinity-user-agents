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
			Err(Rejection::FormatMismatch { declared: ImageFormat::Png, actual: ImageFormat::Gif }),
			Err(Rejection::UnsupportedFormat),
			Err(Rejection::UnreadableSize),
			Err(Rejection::TooLarge { bytes: MAX_IMAGE_BYTES + 1 }),
		]
	);
}
