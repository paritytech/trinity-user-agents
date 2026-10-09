use std::{
	collections::VecDeque,
	fs::{self, File},
	io::BufReader,
	path::Path,
};

use anyhow::{Context, Result as AnyResult, bail, ensure};
use image::{ImageReader, Limits};
use truapi::host_logic::sso::pairing::decode_pairing_deeplink;

const MAX_IMAGE_EDGE: usize = 8_192;
const MAX_IMAGE_PIXELS: usize = 24 * 1024 * 1024;
const MAX_IMAGE_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_ALLOC_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LINE_FINDERS: usize = 2_048;
const MAX_FINDER_INTERSECTIONS: usize = 65_536;
const MAX_FINDER_CLUSTERS: usize = 512;
const MAX_GRID_FINDERS: usize = 64;

pub struct RgbaFrame {
	width: usize,
	height: usize,
	pixels: Vec<u8>,
}

impl RgbaFrame {
	pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> AnyResult<Self> {
		validate_rgba_frame(width, height, &pixels)?;
		Ok(Self { width, height, pixels })
	}
}

#[derive(Debug, PartialEq, Eq)]
enum FrameOutcome {
	NoQr,
	NotPairing,
	MultiplePairingCodes,
	PairingDeeplink(String),
}

impl FrameOutcome {
	fn pairing_deeplink(self) -> AnyResult<String> {
		match self {
			Self::NoQr => bail!("no QR code found in the image"),
			Self::NotPairing => bail!("the QR code is not a valid Polkadot pairing request"),
			Self::MultiplePairingCodes => {
				bail!("the image contains multiple Polkadot pairing QR codes")
			},
			Self::PairingDeeplink(deeplink) => Ok(deeplink),
		}
	}
}

#[derive(Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
enum FrameError {
	#[display("image dimensions {width}x{height} exceed the supported limit")]
	Dimensions { width: usize, height: usize },
	#[display("RGBA image contains {actual} bytes, expected {expected}")]
	Length { expected: usize, actual: usize },
}

#[derive(Clone, Copy)]
struct LineFinder {
	line: f64,
	center: f64,
	module: f64,
	foreground: bool,
}

#[derive(Clone, Copy)]
struct Finder {
	x: f64,
	y: f64,
	module: f64,
	foreground: bool,
	matches: usize,
}

pub fn decode_path(path: &Path) -> AnyResult<String> {
	let metadata = fs::metadata(path)
		.with_context(|| format!("read image metadata from {}", path.display()))?;
	ensure!(metadata.is_file(), "{} is not an image file", path.display());
	ensure!(metadata.len() <= MAX_IMAGE_FILE_BYTES, "image file is larger than 64 MiB");

	let file = File::open(path).with_context(|| format!("open image {}", path.display()))?;
	let mut reader = ImageReader::new(BufReader::new(file))
		.with_guessed_format()
		.with_context(|| format!("detect image format for {}", path.display()))?;
	let mut limits = Limits::default();
	limits.max_image_width = Some(MAX_IMAGE_EDGE as u32);
	limits.max_image_height = Some(MAX_IMAGE_EDGE as u32);
	limits.max_alloc = Some(MAX_IMAGE_ALLOC_BYTES);
	reader.limits(limits);
	let image = reader
		.decode()
		.with_context(|| format!("decode image {}", path.display()))?
		.into_rgba8();
	let frame = RgbaFrame::new(image.width() as usize, image.height() as usize, image.into_raw())?;
	decode(&frame)
}

pub fn decode(frame: &RgbaFrame) -> AnyResult<String> {
	decode_rgba_frame(frame.width, frame.height, &frame.pixels)?.pairing_deeplink()
}

fn validate_rgba_frame(width: usize, height: usize, pixels: &[u8]) -> Result<(), FrameError> {
	let pixel_count = width
		.checked_mul(height)
		.filter(|count| {
			width > 0 &&
				height > 0 && width <= MAX_IMAGE_EDGE &&
				height <= MAX_IMAGE_EDGE &&
				*count <= MAX_IMAGE_PIXELS
		})
		.ok_or(FrameError::Dimensions { width, height })?;
	let expected = pixel_count.checked_mul(4).ok_or(FrameError::Dimensions { width, height })?;
	if pixels.len() != expected {
		return Err(FrameError::Length { expected, actual: pixels.len() });
	}
	Ok(())
}

fn decode_rgba_frame(
	width: usize,
	height: usize,
	pixels: &[u8],
) -> Result<FrameOutcome, FrameError> {
	validate_rgba_frame(width, height, pixels)?;
	let grayscale = pixels
		.as_chunks::<4>()
		.0
		.iter()
		.map(|pixel| {
			let luminance =
				(u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19) /
					256;
			((luminance * u32::from(pixel[3]) + 255 * u32::from(255_u8.saturating_sub(pixel[3]))) /
				255) as u8
		})
		.collect::<Vec<_>>();
	let mut payloads = detected_payloads(width, height, &grayscale, false);
	payloads.extend(detected_payloads(width, height, &grayscale, true));
	payloads.extend(axis_aligned_payloads(width, height, &grayscale));
	payloads.sort();
	payloads.dedup();

	let decoded_any = !payloads.is_empty();
	let mut pairing_deeplinks = payloads
		.into_iter()
		.filter(|payload| {
			payload.starts_with("polkadotapp://pair?handshake=") &&
				decode_pairing_deeplink(payload).is_ok()
		})
		.collect::<Vec<_>>();
	Ok(match pairing_deeplinks.len() {
		0 if decoded_any => FrameOutcome::NotPairing,
		0 => FrameOutcome::NoQr,
		1 => FrameOutcome::PairingDeeplink(pairing_deeplinks.pop().expect("one deeplink")),
		_ => FrameOutcome::MultiplePairingCodes,
	})
}

fn detected_payloads(width: usize, height: usize, pixels: &[u8], inverted: bool) -> Vec<String> {
	let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |column, row| {
		let value = pixels[row * width + column];
		if inverted { 255 - value } else { value }
	});
	image
		.detect_grids()
		.into_iter()
		.filter_map(|grid| grid.decode().ok().map(|(_, payload)| payload))
		.collect()
}

fn axis_aligned_payloads(width: usize, height: usize, pixels: &[u8]) -> Vec<String> {
	let threshold = otsu_threshold(pixels);
	let mut finders = match finder_points(width, height, pixels, threshold) {
		Some(finders) => finders,
		None => return Vec::new(),
	};
	finders.sort_by_key(|finder| std::cmp::Reverse(finder.matches));
	finders.truncate(MAX_GRID_FINDERS);

	let mut payloads = Vec::new();
	for top_left in &finders {
		for top_right in &finders {
			for bottom_left in &finders {
				if let Some(payload) =
					sample_grid(width, height, pixels, threshold, top_left, top_right, bottom_left) &&
					!payloads.contains(&payload)
				{
					payloads.push(payload);
				}
			}
		}
	}
	payloads
}

fn finder_points(width: usize, height: usize, pixels: &[u8], threshold: u8) -> Option<Vec<Finder>> {
	let horizontal =
		finder_lines(height, width, |row, column| pixels[row * width + column] > threshold)?;
	let vertical =
		finder_lines(width, height, |column, row| pixels[row * width + column] > threshold)?;
	let mut vertical_by_column = vec![Vec::new(); width];
	for finder in vertical {
		vertical_by_column[finder.line as usize].push(finder);
	}
	let mut finders: Vec<Finder> = Vec::new();
	let mut intersections = 0;
	for horizontal in &horizontal {
		let radius = horizontal.module.ceil() as usize;
		let center = horizontal.center.round() as usize;
		let first_column = center.saturating_sub(radius);
		let last_column = center.saturating_add(radius).min(width - 1);
		for vertical in vertical_by_column[first_column..=last_column].iter().flatten() {
			let module = (horizontal.module + vertical.module) / 2.0;
			if horizontal.foreground != vertical.foreground ||
				(horizontal.module - vertical.module).abs() > module * 0.35 ||
				(horizontal.center - vertical.line).abs() > module ||
				(horizontal.line - vertical.center).abs() > module
			{
				continue;
			}
			intersections += 1;
			if intersections > MAX_FINDER_INTERSECTIONS {
				return None;
			}
			let x = (horizontal.center + vertical.line) / 2.0;
			let y = (horizontal.line + vertical.center) / 2.0;
			if let Some(existing) = finders.iter_mut().find(|existing| {
				existing.foreground == horizontal.foreground &&
					(existing.x - x).abs() < module * 2.0 &&
					(existing.y - y).abs() < module * 2.0
			}) {
				let matches = existing.matches as f64;
				existing.x = (existing.x * matches + x) / (matches + 1.0);
				existing.y = (existing.y * matches + y) / (matches + 1.0);
				existing.module = (existing.module * matches + module) / (matches + 1.0);
				existing.matches += 1;
			} else {
				if finders.len() == MAX_FINDER_CLUSTERS {
					return None;
				}
				finders.push(Finder {
					x,
					y,
					module,
					foreground: horizontal.foreground,
					matches: 1,
				});
			}
		}
	}
	finders.retain(|finder| finder.matches >= 3);
	Some(finders)
}

fn finder_lines<F>(line_count: usize, line_length: usize, mut pixel: F) -> Option<Vec<LineFinder>>
where
	F: FnMut(usize, usize) -> bool,
{
	let mut finders = Vec::new();
	for line in 0..line_count {
		let mut runs = VecDeque::with_capacity(5);
		let mut start = 0;
		for index in 1..=line_length {
			if index < line_length && pixel(line, index) == pixel(line, start) {
				continue;
			}
			runs.push_back((start, index - start, pixel(line, start)));
			start = index;
			if runs.len() < 5 {
				continue;
			}
			let total = runs.iter().map(|(_, length, _)| *length).sum::<usize>();
			let module = total as f64 / 7.0;
			let units = [1.0, 1.0, 3.0, 1.0, 1.0];
			if module >= 2.0 &&
				runs.iter().zip(units).all(|((_, length, _), units)| {
					(*length as f64 - module * units).abs() <= module * 0.65
				}) {
				if finders.len() == MAX_LINE_FINDERS {
					return None;
				}
				finders.push(LineFinder {
					line: line as f64,
					center: runs[0].0 as f64 + total as f64 / 2.0,
					module,
					foreground: runs[0].2,
				});
			}
			runs.pop_front();
		}
	}
	Some(finders)
}

fn sample_grid(
	width: usize,
	height: usize,
	pixels: &[u8],
	threshold: u8,
	top_left: &Finder,
	top_right: &Finder,
	bottom_left: &Finder,
) -> Option<String> {
	let module = (top_left.module + top_right.module + bottom_left.module) / 3.0;
	if top_left.foreground != top_right.foreground ||
		top_left.foreground != bottom_left.foreground ||
		top_right.x <= top_left.x ||
		bottom_left.y <= top_left.y ||
		(top_right.y - top_left.y).abs() > module * 2.0 ||
		(bottom_left.x - top_left.x).abs() > module * 2.0
	{
		return None;
	}

	let horizontal_size = (top_right.x - top_left.x) / module + 7.0;
	let vertical_size = (bottom_left.y - top_left.y) / module + 7.0;
	let version = ((((horizontal_size + vertical_size) / 2.0) - 17.0) / 4.0).round();
	if !(1.0..=40.0).contains(&version) {
		return None;
	}
	let size = version as usize * 4 + 17;
	if (horizontal_size - size as f64).abs() > 1.0 || (vertical_size - size as f64).abs() > 1.0 {
		return None;
	}

	let scale_x = (top_right.x - top_left.x) / (size - 7) as f64;
	let scale_y = (bottom_left.y - top_left.y) / (size - 7) as f64;
	let origin_x = top_left.x - scale_x * 3.5;
	let origin_y = top_left.y - scale_y * 3.5;
	if origin_x < 0.0 ||
		origin_y < 0.0 ||
		origin_x + size as f64 * scale_x >= width as f64 ||
		origin_y + size as f64 * scale_y >= height as f64
	{
		return None;
	}

	let grid = rqrr::SimpleGrid::from_func(size, |column, row| {
		let x = (origin_x + (column as f64 + 0.5) * scale_x).round() as usize;
		let y = (origin_y + (row as f64 + 0.5) * scale_y).round() as usize;
		(pixels[y * width + x] > threshold) == top_left.foreground
	});
	rqrr::Grid::new(grid).decode().ok().map(|(_, payload)| payload)
}

fn otsu_threshold(pixels: &[u8]) -> u8 {
	let mut histogram = [0_u64; 256];
	for pixel in pixels {
		histogram[*pixel as usize] += 1;
	}
	let total = pixels.len() as u64;
	let sum = histogram
		.iter()
		.enumerate()
		.map(|(value, count)| value as u64 * count)
		.sum::<u64>();
	let mut background_count = 0_u64;
	let mut background_sum = 0_u64;
	let mut best_variance = 0.0;
	let mut threshold = 128;
	for (value, count) in histogram.iter().enumerate() {
		background_count += count;
		if background_count == 0 || background_count == total {
			continue;
		}
		background_sum += value as u64 * count;
		let foreground_count = total - background_count;
		let background_mean = background_sum as f64 / background_count as f64;
		let foreground_mean = (sum - background_sum) as f64 / foreground_count as f64;
		let variance = background_count as f64 *
			foreground_count as f64 *
			(background_mean - foreground_mean).powi(2);
		if variance > best_variance {
			best_variance = variance;
			threshold = value as u8;
		}
	}
	threshold
}

#[cfg(test)]
mod tests {
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
					let value =
						if distance < 1.5 || (2.5..3.5).contains(&distance) { 245 } else { 24 };
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
}
