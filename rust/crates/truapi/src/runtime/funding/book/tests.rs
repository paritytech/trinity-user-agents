use super::*;

#[test]
fn an_overlay_closing_drops_its_state() {
	let book = FundingSessionBook::default();
	book.options_changed(
		"fs_1",
		FundingOptions {
			candidates: Vec::new(),
			countries: Vec::new(),
			limits: Default::default(),
			networks: Vec::new(),
			rails: Vec::new(),
			asset: None,
			tokens: Vec::new(),
			detected_country: None,
		},
	);
	let before = book.options("fs_1").is_some();
	book.overlay_closed("fs_1");
	assert_eq!((before, book.options("fs_1")), (true, None));
}
