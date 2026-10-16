use crate::Error;
use qrcode::{
	QrCode,
	render::{svg, unicode},
};

fn code(text: &str) -> Result<QrCode, Error> {
	QrCode::new(text.as_bytes()).map_err(|error| Error::QrCode(error.to_string()))
}

/// A QR code of `text` as an SVG document.
pub fn qr_svg(text: &str) -> Result<String, Error> {
	Ok(code(text)?.render::<svg::Color>().min_dimensions(256, 256).build())
}

/// A QR code of `text` drawn with Unicode half blocks, for a terminal.
pub fn qr_terminal(text: &str) -> Result<String, Error> {
	Ok(code(text)?.render::<unicode::Dense1x2>().quiet_zone(true).build())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn text_too_long_for_a_qr_code_is_its_own_error() {
		let too_long = "a".repeat(8_000);
		assert_eq!(qr_svg(&too_long).map_err(|error| error.code()), Err("qr-code"));
		assert!(qr_svg("polkadotapp://stack?url=https%3A%2F%2Fa.example%2F").is_ok());
	}
}
