use parity_scale_codec::Encode;
use qrcode::{Color, QrCode};
use tempfile::tempdir;
use truapi::host_logic::sso::pairing::{
	VersionedHandshakeProposal,
	v2::{Device, MetadataEntry, MetadataKey, Proposal},
};

use super::*;

#[test]
fn decodes_a_light_on_dark_rgba_pairing_qr() {
	let deeplink = pairing_deeplink_for(1);
	let frame = qr_frame(&deeplink, true);

	assert_eq!(
		decode_rgba_frame(frame.width, frame.height, &frame.pixels),
		Ok(FrameOutcome::PairingDeeplink(deeplink))
	);
}

#[test]
fn decodes_the_circular_finder_styling_used_by_polkadot_apps() {
	let deeplink = pairing_deeplink_for(1);
	let frame = circular_finder_frame(&deeplink);
	let grayscale =
		frame.pixels.as_chunks::<4>().0.iter().map(|pixel| pixel[0]).collect::<Vec<_>>();

	assert_eq!(axis_aligned_payloads(frame.width, frame.height, &grayscale), vec![deeplink]);
}

#[test]
fn ignores_incompatible_finder_noise_around_a_pairing_qr() {
	let deeplink = pairing_deeplink_with_metadata(1, 168);
	let frame = with_incompatible_finder_noise(&circular_finder_frame(&deeplink));

	assert_eq!(
		decode_rgba_frame(frame.width, frame.height, &frame.pixels),
		Ok(FrameOutcome::PairingDeeplink(deeplink))
	);
}

#[test]
fn distinguishes_unrelated_qr_codes_from_images_without_a_qr() {
	let unrelated = qr_frame("https://example.com/not-a-pairing-code", false);
	let blank = RgbaFrame::new(320, 240, vec![255; 320 * 240 * 4]).expect("valid frame");

	assert_eq!(
		(
			decode_rgba_frame(unrelated.width, unrelated.height, &unrelated.pixels),
			decode_rgba_frame(blank.width, blank.height, &blank.pixels),
		),
		(Ok(FrameOutcome::NotPairing), Ok(FrameOutcome::NoQr))
	);
}

#[test]
fn rejects_an_image_with_multiple_pairing_requests() {
	let first = qr_frame(&pairing_deeplink_for(1), false);
	let second = qr_frame(&pairing_deeplink_for(9), false);
	let frame = side_by_side(&first, &second);

	assert_eq!(
		decode_rgba_frame(frame.width, frame.height, &frame.pixels),
		Ok(FrameOutcome::MultiplePairingCodes)
	);
}

#[test]
fn validates_dimensions_and_rgba_length_before_scanning() {
	assert_eq!(
		(
			decode_rgba_frame(0, 1, &[]),
			decode_rgba_frame(MAX_IMAGE_EDGE + 1, 1, &[]),
			decode_rgba_frame(2, 2, &[0; 15]),
		),
		(
			Err(FrameError::Dimensions { width: 0, height: 1 }),
			Err(FrameError::Dimensions { width: MAX_IMAGE_EDGE + 1, height: 1 }),
			Err(FrameError::Length { expected: 16, actual: 15 }),
		)
	);
}

#[test]
fn reads_a_png_image_path_and_rejects_oversized_files() {
	let temporary = tempdir().expect("create fixture directory");
	let deeplink = pairing_deeplink_for(1);
	let frame = qr_frame(&deeplink, false);
	let image_path = temporary.path().join("pairing.png");
	image::save_buffer_with_format(
		&image_path,
		&frame.pixels,
		frame.width as u32,
		frame.height as u32,
		image::ColorType::Rgba8,
		image::ImageFormat::Png,
	)
	.expect("write fixture image");
	let oversized_path = temporary.path().join("oversized.png");
	File::create(&oversized_path)
		.expect("create oversized fixture")
		.set_len(MAX_IMAGE_FILE_BYTES + 1)
		.expect("size oversized fixture");

	assert_eq!(decode_path(&image_path).expect("decode fixture path"), deeplink);
	assert_eq!(
		decode_path(&oversized_path).expect_err("oversized image must fail").to_string(),
		"image file is larger than 64 MiB"
	);
}

fn pairing_deeplink_for(account_byte: u8) -> String {
	pairing_deeplink_with_metadata(account_byte, 0)
}

fn pairing_deeplink_with_metadata(account_byte: u8, metadata_len: usize) -> String {
	let metadata = (0..metadata_len)
		.map(|index| char::from(b'!' + ((index * 47 + 11) % 90) as u8))
		.collect();
	let proposal = VersionedHandshakeProposal::V2(Proposal {
		device: Device {
			statement_account_id: std::array::from_fn(|index| {
				account_byte.wrapping_add((index * 17) as u8)
			}),
			encryption_public_key: std::array::from_fn(|index| {
				account_byte.wrapping_add((index * 29 + 1) as u8)
			}),
		},
		metadata: (metadata_len > 0)
			.then_some(MetadataEntry(MetadataKey::HostName, metadata))
			.into_iter()
			.collect(),
	});
	format!("polkadotapp://pair?handshake={}", hex::encode(proposal.encode()))
}

fn qr_frame(payload: &str, inverted: bool) -> RgbaFrame {
	const QUIET_ZONE: usize = 4;
	const SCALE: usize = 6;

	let code = QrCode::new(payload.as_bytes()).expect("fixture QR encodes");
	let modules = code.to_colors();
	let module_width = code.width();
	let width = (module_width + QUIET_ZONE * 2) * SCALE;
	let background = if inverted { 24 } else { 255 };
	let foreground = if inverted { 245 } else { 0 };
	let mut pixels = vec![background; width * width * 4];
	for alpha in pixels.iter_mut().skip(3).step_by(4) {
		*alpha = 255;
	}
	for row in 0..module_width {
		for column in 0..module_width {
			if modules[row * module_width + column] != Color::Dark {
				continue;
			}
			for output_row in (row + QUIET_ZONE) * SCALE..(row + QUIET_ZONE + 1) * SCALE {
				for output_column in
					(column + QUIET_ZONE) * SCALE..(column + QUIET_ZONE + 1) * SCALE
				{
					let offset = (output_row * width + output_column) * 4;
					pixels[offset..offset + 3].fill(foreground);
				}
			}
		}
	}
	RgbaFrame::new(width, width, pixels).expect("valid QR frame")
}

fn circular_finder_frame(payload: &str) -> RgbaFrame {
	const QUIET_ZONE: usize = 4;
	const SCALE: usize = 6;

	let code = QrCode::new(payload.as_bytes()).expect("fixture QR encodes");
	let module_width = code.width();
	let mut frame = qr_frame(payload, true);
	for (finder_column, finder_row) in [
		(QUIET_ZONE, QUIET_ZONE),
		(QUIET_ZONE + module_width - 7, QUIET_ZONE),
		(QUIET_ZONE, QUIET_ZONE + module_width - 7),
	] {
		let center_x = (finder_column * SCALE) as f64 + 3.5 * SCALE as f64;
		let center_y = (finder_row * SCALE) as f64 + 3.5 * SCALE as f64;
		for row in finder_row * SCALE..(finder_row + 7) * SCALE {
			for column in finder_column * SCALE..(finder_column + 7) * SCALE {
				let distance = ((column as f64 + 0.5 - center_x).powi(2) +
					(row as f64 + 0.5 - center_y).powi(2))
				.sqrt() / SCALE as f64;
				let value = if distance < 1.5 || (2.5..3.5).contains(&distance) { 245 } else { 24 };
				let offset = (row * frame.width + column) * 4;
				frame.pixels[offset..offset + 3].fill(value);
			}
		}
	}
	frame
}

fn with_incompatible_finder_noise(frame: &RgbaFrame) -> RgbaFrame {
	const QR_LEFT: usize = 150;
	const QR_TOP: usize = 1_000;
	const HORIZONTAL_ROWS: usize = 63;
	const HORIZONTAL_START: usize = 30;
	const HORIZONTAL_MODULE: usize = 10;
	const VERTICAL_FIRST_COLUMN: usize = 55;
	const VERTICAL_COLUMNS: usize = 21;
	const VERTICAL_START: usize = 100;
	const VERTICAL_PATTERNS: usize = 50;
	const VERTICAL_PATTERN_HEIGHT: usize = 16;

	let width = QR_LEFT + frame.width;
	let height = QR_TOP + frame.height;
	let mut pixels = vec![24; width * height * 4];
	for alpha in pixels.iter_mut().skip(3).step_by(4) {
		*alpha = 255;
	}
	for row in 0..HORIZONTAL_ROWS {
		for run in [(0, 1), (2, 5), (6, 7)] {
			for column in HORIZONTAL_START + run.0 * HORIZONTAL_MODULE..
				HORIZONTAL_START + run.1 * HORIZONTAL_MODULE
			{
				let offset = (row * width + column) * 4;
				pixels[offset..offset + 3].fill(245);
			}
		}
	}
	for column in VERTICAL_FIRST_COLUMN..VERTICAL_FIRST_COLUMN + VERTICAL_COLUMNS {
		for pattern in 0..VERTICAL_PATTERNS {
			let start = VERTICAL_START + pattern * VERTICAL_PATTERN_HEIGHT;
			for row in [start..start + 2, start + 4..start + 10, start + 12..start + 14]
				.into_iter()
				.flatten()
			{
				let offset = (row * width + column) * 4;
				pixels[offset..offset + 3].fill(245);
			}
		}
	}
	for row in 0..frame.height {
		let source = row * frame.width * 4;
		let destination = ((row + QR_TOP) * width + QR_LEFT) * 4;
		pixels[destination..destination + frame.width * 4]
			.copy_from_slice(&frame.pixels[source..source + frame.width * 4]);
	}
	RgbaFrame::new(width, height, pixels).expect("valid noisy QR frame")
}

fn side_by_side(left: &RgbaFrame, right: &RgbaFrame) -> RgbaFrame {
	const GAP: usize = 32;

	let width = left.width + GAP + right.width;
	let height = left.height.max(right.height);
	let mut pixels = vec![255; width * height * 4];
	for alpha in pixels.iter_mut().skip(3).step_by(4) {
		*alpha = 255;
	}
	for (frame, column_offset) in [(left, 0), (right, left.width + GAP)] {
		for row in 0..frame.height {
			let source = row * frame.width * 4;
			let destination = (row * width + column_offset) * 4;
			pixels[destination..destination + frame.width * 4]
				.copy_from_slice(&frame.pixels[source..source + frame.width * 4]);
		}
	}
	RgbaFrame::new(width, height, pixels).expect("valid combined frame")
}
