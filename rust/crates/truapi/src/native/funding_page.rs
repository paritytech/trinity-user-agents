//! What a host's web view lets a provider's external page do: when it
//! reached its return URL, where it may navigate, what origin it is on, and
//! who may use the camera and microphone. Every answer comes from the URLs
//! given.

use url::Url;

/// A page's origin.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FundingPageOrigin {
	/// The scheme, lower-case.
	pub scheme: String,
	/// The host, lower-case.
	pub host: String,
	/// The port, when it is not the scheme's default.
	pub port: Option<u16>,
}

/// Whether a provider page at `url` reached its `return_url`.
///
/// It did with the same scheme and host in any case, the same port once a
/// default one is filled in, and the same path, an empty one read as `/`.
/// The query and fragment are the provider's result and are ignored.
#[uniffi::export]
pub fn funding_page_matches_return(url: String, return_url: String) -> bool {
	let (Ok(url), Ok(expected)) = (Url::parse(&url), Url::parse(&return_url)) else {
		return false;
	};
	url.scheme() == expected.scheme() &&
		same_host_and_port(&url, &expected) &&
		path(&url) == path(&expected)
}

/// Whether a provider page's web view may load `url`.
///
/// Any https page may load, and in a sub-frame also the `about:`, `blob:`
/// and `data:` documents pages build themselves. Nothing else, so a page
/// cannot leave https.
#[uniffi::export]
pub fn funding_page_allows_navigation(url: String, is_main_frame: bool) -> bool {
	match Url::parse(&url).as_ref().map(Url::scheme) {
		Ok("https") => true,
		Ok("about" | "blob" | "data") => !is_main_frame,
		_ => false,
	}
}

/// The origin of `url`, normalised: lower-case scheme and host, and the
/// port only when it is not the scheme's default. `None` without a host.
#[uniffi::export]
pub fn funding_page_origin(url: String) -> Option<FundingPageOrigin> {
	let parsed = Url::parse(&url).ok()?;
	Some(FundingPageOrigin {
		scheme: parsed.scheme().to_string(),
		host: parsed.host_str()?.to_ascii_lowercase(),
		port: parsed.port(),
	})
}

/// Whether `origin` may use the camera and microphone on the provider page
/// loaded at `page_url`: only that https page's own origin.
#[uniffi::export]
pub fn funding_page_allows_media(page_url: String, origin: String) -> bool {
	let (Ok(page), Ok(origin)) = (Url::parse(&page_url), Url::parse(&origin)) else {
		return false;
	};
	page.scheme() == "https" &&
		origin.scheme() == "https" &&
		page.host_str().is_some() &&
		same_host_and_port(&page, &origin)
}

/// Whether `a` and `b` name the same host in any case, on the same port
/// once a default one is filled in.
fn same_host_and_port(a: &Url, b: &Url) -> bool {
	a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase) &&
		a.port_or_known_default() == b.port_or_known_default()
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
