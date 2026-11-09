//! What a host's web view lets a provider's external page do: when it
//! reached its return URL, where it may navigate, what origin it is on, and
//! who may use the camera and microphone.

use url::Url;

/// A page's origin.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingPageOrigin {
	/// The scheme, lower-case.
	pub scheme: String,
	/// The host, lower-case.
	pub host: String,
	/// The port, when it is not the scheme's default.
	pub port: Option<u16>,
}

/// The rules a provider page's web view follows. Stateless: every answer
/// comes from the URLs it is given.
pub struct FundingPagePolicy;

impl FundingPagePolicy {
	/// Whether `url` is the page's `return_url`: the same scheme and host in
	/// any case, the same port once a default one is filled in, and the same
	/// path, an empty one read as `/`. The query and fragment are the
	/// provider's result and are ignored.
	pub fn matches_return(url: &str, return_url: &str) -> bool {
		let (Ok(url), Ok(expected)) = (Url::parse(url), Url::parse(return_url)) else {
			return false;
		};
		url.scheme() == expected.scheme() &&
			url.host_str().map(str::to_ascii_lowercase) ==
				expected.host_str().map(str::to_ascii_lowercase) &&
			url.port_or_known_default() == expected.port_or_known_default() &&
			path(&url) == path(&expected)
	}

	/// Whether the web view may load `url`: any https page, and in a
	/// sub-frame also the `about:`, `blob:` and `data:` documents pages build
	/// themselves. Nothing else, so a page cannot leave https.
	pub fn allows_navigation(url: &str, is_main_frame: bool) -> bool {
		match Url::parse(url).as_ref().map(Url::scheme) {
			Ok("https") => true,
			Ok("about" | "blob" | "data") => !is_main_frame,
			_ => false,
		}
	}

	/// The origin of `url`, normalised: lower-case scheme and host, and the
	/// port only when it is not the scheme's default. `None` without a host.
	pub fn origin(url: &str) -> Option<FundingPageOrigin> {
		let parsed = Url::parse(url).ok()?;
		Some(FundingPageOrigin {
			scheme: parsed.scheme().to_string(),
			host: parsed.host_str()?.to_ascii_lowercase(),
			port: parsed.port(),
		})
	}

	/// Whether `origin` may use the camera and microphone on `page_url`:
	/// only the https page loaded in the view, on its own origin.
	pub fn allows_media(page_url: &str, origin: &str) -> bool {
		let (Ok(page), Ok(origin)) = (Url::parse(page_url), Url::parse(origin)) else {
			return false;
		};
		page.scheme() == "https" &&
			origin.scheme() == "https" &&
			page.host_str().is_some() &&
			page.host_str().map(str::to_ascii_lowercase) ==
				origin.host_str().map(str::to_ascii_lowercase) &&
			page.port_or_known_default() == origin.port_or_known_default()
	}
}

/// `url`'s path as written, `/` when empty.
fn path(url: &Url) -> &str {
	match url.path() {
		"" => "/",
		path => path,
	}
}

#[cfg(test)]
mod tests;
