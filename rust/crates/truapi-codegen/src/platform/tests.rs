use std::collections::HashMap;

use serde_json::json;

use crate::rustdoc::ItemPath;

use super::*;

#[test]
fn extract_async_trait_future_output_from_pin_box_dyn_future() {
	let output = json!({
		"resolved_path": {
			"path": "::core::pin::Pin",
			"args": {
				"angle_bracketed": {
					"args": [
						{
							"type": {
								"resolved_path": {
									"path": "Box",
									"args": {
										"angle_bracketed": {
											"args": [
												{
													"type": {
														"dyn_trait": {
															"traits": [
																{
																	"trait": {
																		"path": "::core::future::Future",
																		"args": {
																			"angle_bracketed": {
																				"args": [],
																				"constraints": [
																					{
																						"name": "Output",
																						"binding": {
																							"equality": {
																								"type": { "primitive": "u8" }
																							}
																						}
																					}
																				]
																			}
																		}
																	}
																},
																{
																	"trait": {
																		"path": "::core::marker::Send",
																		"args": null
																	}
																}
															],
															"lifetime": "'async_trait"
														}
													}
												}
											],
											"constraints": []
										}
									}
								}
							}
						}
					],
					"constraints": []
				}
			}
		}
	});

	assert_eq!(extract_async_trait_future_output(&output), Some(json!({ "primitive": "u8" })));
}

// Server-internal traits such as `Transport` share the rustdoc JSON with
// the platform surface; only the `platform` module is host-facing.
#[test]
fn only_traits_in_the_platform_module_are_capabilities() {
	let item_path = |crate_id: u32, path: &[&str]| ItemPath {
		crate_id,
		path: path.iter().map(|segment| segment.to_string()).collect(),
		kind: "trait".to_string(),
	};
	let krate = Crate {
		format_version: Some(57),
		index: HashMap::new(),
		paths: HashMap::from([
			("1".to_string(), item_path(0, &["truapi", "platform", "Navigation"])),
			("2".to_string(), item_path(0, &["truapi", "transport", "Transport"])),
			("3".to_string(), item_path(0, &["truapi", "Platform"])),
			("4".to_string(), item_path(1, &["truapi_provider", "platform", "ChainProvider"])),
		]),
	};

	assert_eq!(collect_local_trait_ids(&krate), BTreeSet::from(["1".to_string()]));
}

#[test]
fn referenced_platform_type_names_must_be_unambiguous() {
	let krate = Crate {
		format_version: Some(57),
		index: HashMap::new(),
		paths: HashMap::from([
			(
				"1".to_string(),
				ItemPath {
					crate_id: 0,
					path: vec![
						"truapi".to_string(),
						"platform".to_string(),
						"one".to_string(),
						"Shared".to_string(),
					],
					kind: "struct".to_string(),
				},
			),
			(
				"2".to_string(),
				ItemPath {
					crate_id: 0,
					path: vec![
						"truapi".to_string(),
						"platform".to_string(),
						"two".to_string(),
						"Shared".to_string(),
					],
					kind: "enum".to_string(),
				},
			),
		]),
	};
	let traits = [PlatformTrait {
		name: "Storage".to_string(),
		docs: None,
		methods: vec![PlatformMethod {
			name: "write".to_string(),
			docs: None,
			params: vec![PlatformParam {
				name: "value".to_string(),
				type_ref: TypeRef::Named { name: "Shared".to_string(), args: Vec::new() },
				borrowed: false,
			}],
			return_shape: PlatformReturn { is_async: false, inner: PlatformInner::Unit },
			has_default: false,
		}],
	}];

	let err = collect_referenced_local_types(&krate, &traits, &NameContext::default()).unwrap_err();
	let msg = err.to_string();
	assert!(msg.contains("platform type name `Shared` is ambiguous"), "unexpected error: {msg}");
	assert!(
		msg.contains("truapi::platform::one::Shared") &&
			msg.contains("truapi::platform::two::Shared"),
		"unexpected error: {msg}"
	);
}
