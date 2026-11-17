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

#[derive(Debug, PartialEq)]
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

#[derive(derive_more::Display, derive_more::Error, Debug, PartialEq)]
enum FrameError {
	#[display("image dimensions {width}x{height} exceed the supported limit")]
	Dimensions { width: usize, height: usize },
	#[display("RGBA image contains {actual} bytes, expected {expected}")]
	Length { expected: usize, actual: usize },
}

fn to_f64(value: usize) -> f64 {
	f64::from(u32::try_from(value).expect("image coordinates are bounded by MAX_IMAGE_EDGE"))
}

fn count_to_f64(count: u64) -> f64 {
	f64::from(u32::try_from(count).expect("pixel counts are bounded by MAX_IMAGE_PIXELS"))
}

#[derive(Clone)]
struct LineFinder {
	line: usize,
	center: f64,
	module: f64,
	foreground: bool,
}

#[derive(Clone)]
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
	let max_edge = u32::try_from(MAX_IMAGE_EDGE).expect("MAX_IMAGE_EDGE fits in u32");
	limits.max_image_width = Some(max_edge);
	limits.max_image_height = Some(max_edge);
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
			u8::try_from(
				(luminance * u32::from(pixel[3]) +
					255 * u32::from(255_u8.saturating_sub(pixel[3]))) /
					255,
			)
			.expect("an alpha-weighted luminance stays in u8")
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
		vertical_by_column[finder.line].push(finder);
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
				(horizontal.center - to_f64(vertical.line)).abs() > module ||
				(to_f64(horizontal.line) - vertical.center).abs() > module
			{
				continue;
			}
			intersections += 1;
			if intersections > MAX_FINDER_INTERSECTIONS {
				return None;
			}
			let x = (horizontal.center + to_f64(vertical.line)) / 2.0;
			let y = (to_f64(horizontal.line) + vertical.center) / 2.0;
			if let Some(existing) = finders.iter_mut().find(|existing| {
				existing.foreground == horizontal.foreground &&
					(existing.x - x).abs() < module * 2.0 &&
					(existing.y - y).abs() < module * 2.0
			}) {
				let matches = to_f64(existing.matches);
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
			let module = to_f64(total) / 7.0;
			let units = [1.0, 1.0, 3.0, 1.0, 1.0];
			if module >= 2.0 &&
				runs.iter().zip(units).all(|((_, length, _), units)| {
					(to_f64(*length) - module * units).abs() <= module * 0.65
				}) {
				if finders.len() == MAX_LINE_FINDERS {
					return None;
				}
				finders.push(LineFinder {
					line,
					center: to_f64(runs[0].0) + to_f64(total) / 2.0,
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
	if (horizontal_size - to_f64(size)).abs() > 1.0 || (vertical_size - to_f64(size)).abs() > 1.0 {
		return None;
	}

	let scale_x = (top_right.x - top_left.x) / to_f64(size - 7);
	let scale_y = (bottom_left.y - top_left.y) / to_f64(size - 7);
	let origin_x = top_left.x - scale_x * 3.5;
	let origin_y = top_left.y - scale_y * 3.5;
	if origin_x < 0.0 ||
		origin_y < 0.0 ||
		origin_x + to_f64(size) * scale_x >= to_f64(width) ||
		origin_y + to_f64(size) * scale_y >= to_f64(height)
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
		let background_mean = count_to_f64(background_sum) / count_to_f64(background_count);
		let foreground_mean = count_to_f64(sum - background_sum) / count_to_f64(foreground_count);
		let variance = count_to_f64(background_count) *
			count_to_f64(foreground_count) *
			(background_mean - foreground_mean).powi(2);
		if variance > best_variance {
			best_variance = variance;
			threshold = u8::try_from(value).expect("the histogram has 256 buckets");
		}
	}
	threshold
}

#[cfg(test)]
mod tests;
