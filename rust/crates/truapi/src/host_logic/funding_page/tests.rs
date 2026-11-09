use super::*;

#[test]
fn a_return_url_matches_by_scheme_host_port_and_path() {
	let expected = "https://pay.example/done";
	let cases = [
		("https://pay.example/done?status=ok#top", true),
		("https://PAY.example:443/done", true),
		("https://pay.example:8443/done", false),
		("https://pay.example/done/", false),
		("http://pay.example/done", false),
		("https://other.example/done", false),
		("not a url", false),
	];
	for (url, matches) in cases {
		assert_eq!(FundingPagePolicy::matches_return(url, expected), matches, "{url}");
	}
	assert!(FundingPagePolicy::matches_return("https://pay.example", "https://pay.example/"));
}

// Divergence "Page sub-frame about:/blob:/data:": allowed, in sub-frames
// only; the main frame stays on https.
#[test]
fn a_page_navigates_on_https_and_builds_sub_frames_itself() {
	let cases = [
		("https://pay.example/", true, true),
		("https://pay.example/", false, true),
		("about:blank", false, true),
		("about:blank", true, false),
		("blob:https://pay.example/uuid", false, true),
		("data:text/html,hi", false, true),
		("data:text/html,hi", true, false),
		("http://pay.example/", false, false),
		("javascript:alert(1)", false, false),
	];
	for (url, main, allowed) in cases {
		assert_eq!(FundingPagePolicy::allows_navigation(url, main), allowed, "{url} main={main}");
	}
}

#[test]
fn an_origin_is_normalised() {
	assert_eq!(
		FundingPagePolicy::origin("https://Pay.Example:443/x"),
		Some(FundingPageOrigin {
			scheme: "https".to_string(),
			host: "pay.example".to_string(),
			port: None
		})
	);
	assert_eq!(
		FundingPagePolicy::origin("https://pay.example:8443/").map(|o| o.port),
		Some(Some(8443))
	);
	assert_eq!(FundingPagePolicy::origin("about:blank"), None);
}

// Divergence "Media grant": only the on-screen page's own origin.
#[test]
fn media_goes_only_to_the_page_on_screen() {
	let page = "https://kyc.example/verify";
	let cases = [
		("https://kyc.example", true),
		("https://KYC.example:443", true),
		("https://kyc.example:8443", false),
		("https://ads.example", false),
		("http://kyc.example", false),
	];
	for (origin, allowed) in cases {
		assert_eq!(FundingPagePolicy::allows_media(page, origin), allowed, "{origin}");
	}
	assert!(!FundingPagePolicy::allows_media("http://kyc.example/", "http://kyc.example"));
}
