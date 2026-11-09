//! Stateless funding rules native hosts call, so both apps treat amounts,
//! fees and statuses the same way.

use std::path::Path;

use truapi::latest::{FundingDirection, FundingQuote};

use super::funding_feed::resume_marker;
use crate::{
	host_logic::{
		bundled_products::{self, BundleFiles, BundledProduct},
		funding::FundingStatus,
		funding_page::{FundingPageOrigin, FundingPagePolicy},
		funding_providers::{FundingAmountLimits, FundingProviderEntry},
		funding_view::{self, FundingFeeBreakdown},
	},
	platform::{FundingAmountIssue, FundingQuoteList},
};

/// Why `amount` (balance units) cannot go ahead, if it cannot: more than
/// `spendable` on a withdrawal, then what every provider's refusal of this
/// same amount in `quotes` said, then the amount limits from the session's
/// options.
#[uniffi::export]
pub fn funding_amount_issue(
	direction: FundingDirection,
	limits: FundingAmountLimits,
	quotes: Option<FundingQuoteList>,
	amount: u128,
	spendable: Option<u128>,
) -> Option<FundingAmountIssue> {
	funding_view::amount_issue(
		direction,
		&limits,
		quotes.as_ref().map(|quotes| (quotes.amount, quotes.issue.as_ref())),
		amount,
		spendable,
	)
}

/// What `quote`'s fees come to in `direction`, with a withdrawal's fee
/// also rounded up to the cent, as it is charged.
#[uniffi::export]
pub fn funding_fee_breakdown(
	quote: FundingQuote,
	direction: FundingDirection,
) -> FundingFeeBreakdown {
	funding_view::fee_breakdown(&quote, direction)
}

/// `text`, an amount typed with `.` as its separator, in units of
/// `decimals` decimals, rounded down. `None` when it is not a plain decimal
/// number or does not fit.
#[uniffi::export]
pub fn funding_units(text: String, decimals: u8) -> Option<u128> {
	funding_view::units(&text, decimals)
}

/// Whether asset `code` is a fiat currency, by ISO 4217, rather than a
/// crypto asset.
#[uniffi::export]
pub fn funding_asset_is_fiat(code: String) -> bool {
	funding_view::asset_is_fiat(&code)
}

/// The canonical English name of a crypto network id, which hosts may
/// localise.
#[uniffi::export]
pub fn funding_network_name(id: String) -> String {
	funding_view::network_name(&id)
}

/// Whether a session at `status` is still in flight.
#[uniffi::export]
pub fn funding_status_in_flight(status: FundingStatus) -> bool {
	funding_view::status_in_flight(&status)
}

/// Whether a session at `status` is held up: retrying, taking longer, or
/// waiting on a cancel the provider answered.
#[uniffi::export]
pub fn funding_status_delayed(status: FundingStatus) -> bool {
	funding_view::status_delayed(&status)
}

/// Read access to the products a host bundles in its app, by `/`-separated
/// path under the bundle root.
#[uniffi::export(callback_interface)]
pub trait BundleReader: Send + Sync {
	/// The names directly inside directory `dir`; empty when it is not one.
	fn list(&self, dir: String) -> Vec<String>;
	/// Whether a file is at `path`.
	fn exists(&self, path: String) -> bool;
	/// The UTF-8 text of the file at `path`, if it is one.
	fn read_text(&self, path: String) -> Option<String>;
}

impl BundleFiles for dyn BundleReader {
	fn list(&self, dir: &str) -> Vec<String> {
		BundleReader::list(self, dir.to_string())
	}

	fn exists(&self, path: &str) -> bool {
		BundleReader::exists(self, path.to_string())
	}

	fn read_text(&self, path: &str) -> Option<String> {
		BundleReader::read_text(self, path.to_string())
	}
}

/// The products bundled under `root`, by id: each non-hidden, non-empty
/// directory named by a product id, with whether it ships a Worker and an
/// App.
#[uniffi::export]
pub fn bundled_products(root: String, reader: Box<dyn BundleReader>) -> Vec<BundledProduct> {
	bundled_products::bundled_products(&root, reader.as_ref())
}

/// The funding providers bundled under `root`: each product shipping a
/// Worker whose manifest serves Funding, as an entry for
/// `set_funding_provider_config` with its manifest and whether it ships an
/// App. A manifest that does not parse leaves its product out, logged.
#[uniffi::export]
pub fn bundled_funding_providers(
	root: String,
	reader: Box<dyn BundleReader>,
) -> Vec<FundingProviderEntry> {
	bundled_products::bundled_funding_providers(&root, reader.as_ref())
}

/// The file under a bundled product's `root` that serves `request_path`, or
/// `None` for none: see the core's bundled asset rules.
#[uniffi::export]
pub fn bundled_asset_path(
	root: String,
	request_path: String,
	is_main_frame: bool,
	reader: Box<dyn BundleReader>,
) -> Option<String> {
	bundled_products::bundled_asset_path(&root, &request_path, is_main_frame, reader.as_ref())
}

/// The MIME type a bundled file at `path` is served with.
#[uniffi::export]
pub fn mime_for(path: String) -> String {
	bundled_products::mime_for(&path).to_string()
}

/// Whether a provider page at `url` reached its `return_url`: the same
/// scheme, host, effective port and path; query and fragment ignored.
#[uniffi::export]
pub fn funding_page_matches_return(url: String, return_url: String) -> bool {
	FundingPagePolicy::matches_return(&url, &return_url)
}

/// Whether a provider page's web view may load `url`: https, and in a
/// sub-frame also `about:`, `blob:` and `data:`.
#[uniffi::export]
pub fn funding_page_allows_navigation(url: String, is_main_frame: bool) -> bool {
	FundingPagePolicy::allows_navigation(&url, is_main_frame)
}

/// The normalised origin of `url`, `None` without a host.
#[uniffi::export]
pub fn funding_page_origin(url: String) -> Option<FundingPageOrigin> {
	FundingPagePolicy::origin(&url)
}

/// Whether `origin` may use the camera and microphone on the provider page
/// loaded at `page_url`: only that https page's own origin.
#[uniffi::export]
pub fn funding_page_allows_media(page_url: String, origin: String) -> bool {
	FundingPagePolicy::allows_media(&page_url, &origin)
}

/// Whether a provider still worked on a funding session when the runtime
/// over `database_directory` last ran, so the host starts it at launch to
/// let funding carry on. Read before any runtime is built.
#[uniffi::export]
pub fn funding_resume_pending(database_directory: String) -> bool {
	resume_marker(Path::new(&database_directory)).exists()
}
