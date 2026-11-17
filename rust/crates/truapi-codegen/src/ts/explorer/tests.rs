use super::*;

#[test]
fn category_falls_back_to_shared_for_versioned_module() {
	let path = vec!["truapi".into(), "versioned".into()];
	assert_eq!(data_type_category(&path), "shared");
}

#[test]
fn category_falls_back_to_shared_for_v01_only() {
	let path = vec!["truapi".into(), "v01".into()];
	assert_eq!(data_type_category(&path), "shared");
}

#[test]
fn category_uses_first_non_versioning_segment() {
	let path = vec!["truapi".into(), "api".into(), "account".into()];
	assert_eq!(data_type_category(&path), "account");
}

#[test]
fn category_inside_v0n_module() {
	let path = vec!["truapi".into(), "v02".into(), "account".into()];
	assert_eq!(data_type_category(&path), "account");
}
