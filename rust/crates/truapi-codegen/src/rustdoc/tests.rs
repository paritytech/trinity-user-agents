use super::*;

#[test]
fn product_id_references_retain_nominal_identity() {
	let names = NameContext {
		string_newtypes: HashSet::from(["42".to_string()]),
		..NameContext::default()
	};
	let reference =
		serde_json::json!({"resolved_path": {"path": "truapi::ProductId", "id": 42, "args": null}});
	assert_eq!(
		resolve_type(&reference, &names).unwrap(),
		TypeRef::Named { name: "ProductId".to_string(), args: vec![] }
	);
}

#[test]
fn clean_docs_strips_wire_markers() {
	let docs = "Trait summary.\n\n@wire_id=7\n@wire_trait_id=3\n\
                @wire_host_initiated\n@wire_internal\n\
                @service_required_execution=Chat\n";

	assert_eq!(clean_docs(Some(docs)).as_deref(), Some("Trait summary."));
}

#[test]
fn internal_wire_metadata_preserves_method_address_and_direction() {
	assert_eq!(
		extract_wire_attrs("Method summary.\n@wire_id=2\n@wire_host_initiated\n@wire_internal\n"),
		WireAttrs { host_initiated: true, internal: true, id: Some(2) }
	);
}

#[test]
fn public_methods_require_no_visibility_marker() {
	assert_eq!(
		extract_wire_attrs("Method summary.\n@wire_id=1\n"),
		WireAttrs { id: Some(1), ..WireAttrs::default() }
	);
}

#[test]
fn trait_exposes_required_execution_without_leaking_marker() {
	let trait_def = TraitDef {
		name: "Chat".into(),
		module_path: Vec::new(),
		wire_trait_id: None,
		methods: Vec::new(),
		docs: Some("Chat operations.\n\n@service_required_execution=Chat".into()),
	};

	assert_eq!(trait_def.required_execution(), Some("Chat"));
	assert_eq!(trait_def.public_docs().as_deref(), Some("Chat operations."));
}

#[test]
fn extract_wire_trait_id_reads_marker() {
	assert_eq!(
		extract_wire_trait_id("Theme", "Trait summary.\n\n@wire_trait_id=14\n").unwrap(),
		Some(14)
	);
	assert_eq!(extract_wire_trait_id("Theme", "Trait summary.").unwrap(), None);
}

/// A value the attribute could never emit must fail loudly instead of
/// truncating to a valid id or degrading into "missing annotation".
#[test]
fn extract_wire_trait_id_rejects_malformed_markers() {
	for docs in [
		"@wire_trait_id=300",
		"@wire_trait_id=",
		"@wire_trait_id=12abc",
		"@wire_trait_id=-1",
		"@wire_trait_id=1 2",
	] {
		let err =
			extract_wire_trait_id("Theme", docs).expect_err("malformed marker must be rejected");
		assert!(format!("{err:#}").contains("malformed"), "unexpected error for {docs:?}: {err:#}");
	}
}

/// A hand-written doc line must not be able to outrank the attribute: the
/// proc-macro appends its marker last, so a silent first-wins or last-wins
/// rule would let prose move the trait's whole method block on the wire.
#[test]
fn extract_wire_trait_id_rejects_a_second_marker() {
	let err = extract_wire_trait_id("Theme", "@wire_trait_id=99\n@wire_trait_id=14\n")
		.expect_err("a forged second marker must be rejected");
	assert!(format!("{err:#}").contains("more than one"), "unexpected error: {err:#}");
}

#[test]
fn parse_accepts_tested_format_version() {
	let json = format!(r#"{{ "format_version": {MIN_FORMAT_VERSION}, "index": {{}} }}"#);

	assert!(parse(&json).is_ok());
}

#[test]
fn parse_rejects_missing_format_version() {
	let err = parse(r#"{ "index": {} }"#).expect_err("missing format_version must error");

	assert!(format!("{err}").contains("missing `format_version`"), "unexpected error: {err}");
}

#[test]
fn parse_rejects_old_format_version() {
	let json = format!(r#"{{ "format_version": {}, "index": {{}} }}"#, MIN_FORMAT_VERSION - 1);
	let err = parse(&json).expect_err("old format_version must error");

	assert!(format!("{err}").contains("older than the tested minimum"), "unexpected error: {err}");
}

#[test]
fn host_initiated_requests_accept_async_result_and_require_call_error() {
	let named = |name: &str, args: Vec<serde_json::Value>| {
		serde_json::json!({
			"resolved_path": {
				"path": name,
				"args": { "angle_bracketed": { "args": args.into_iter().map(|ty| serde_json::json!({"type": ty})).collect::<Vec<_>>(), "constraints": [] } }
			}
		})
	};
	let request = named("ProductAnswerRequest", vec![]);
	let response = named("ProductAnswerResponse", vec![]);
	let domain = named("ProductAnswerError", vec![]);
	let call_error = named("CallError", vec![domain.clone()]);
	let method = |error: serde_json::Value| {
		let result = named("Result", vec![response.clone(), error]);
		let future = serde_json::json!({ "dyn_trait": { "traits": [{ "trait": {
			"path": "Future",
			"args": { "angle_bracketed": { "args": [], "constraints": [{
				"name": "Output", "binding": { "equality": { "type": result } }
			}] } }
		} }], "lifetime": "'async_trait" } });
		Item {
			name: Some("answer".into()),
			docs: Some("@wire_host_initiated\n@wire_id=7".into()),
			attrs: Vec::new(),
			inner: serde_json::json!({ "function": { "sig": {
				"output": named("Pin", vec![named("Box", vec![future])]),
				"inputs": [["_cx", { "borrowed_ref": { "type": named("CallContext", vec![]), "is_mutable": false } }], ["request", request]]
			} } }),
		}
	};
	let extracted = extract_method("1", &method(call_error), &NameContext::default())
		.unwrap()
		.unwrap();
	assert_eq!((extracted.kind, extracted.wire.host_initiated), (MethodKind::Request, true));
	let error = extract_method("1", &method(domain), &NameContext::default()).unwrap_err();
	assert!(error.to_string().contains("CallError<E>"), "{error}");
}

#[test]
fn unwraps_async_trait_send_future_output() {
	let output = serde_json::json!({
		"resolved_path": {
			"path": "::core::pin::Pin",
			"args": {
				"angle_bracketed": {
					"args": [{
						"type": {
							"resolved_path": {
								"path": "Box",
								"args": {
									"angle_bracketed": {
										"args": [{
											"type": {
												"dyn_trait": {
													"traits": [
														{
															"trait": {
																"path": "::core::future::Future",
																"args": {
																	"angle_bracketed": {
																		"args": [],
																		"constraints": [{
																			"name": "Output",
																			"binding": {
																				"equality": {
																					"type": {
																						"resolved_path": {
																							"path": "Result",
																							"id": 1,
																							"args": null
																						}
																					}
																				}
																			}
																		}]
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
										}],
										"constraints": []
									}
								}
							}
						}
					}],
					"constraints": []
				}
			}
		}
	});

	let unwrapped = unwrap_future_output(&output);

	assert_eq!(get_resolved_name(unwrapped).as_deref(), Some("Result"));
}

/// A return that is not an `async_trait` future is the method's own type, so
/// it has to pass through untouched. Rejecting it here would turn every
/// non-async method into a parse failure instead of a plain return type.
#[test]
fn a_return_that_is_not_a_future_passes_through() {
	let output = serde_json::json!({
		"resolved_path": { "path": "Result", "id": 1, "args": null }
	});

	assert_eq!(unwrap_future_output(&output), &output);
}

/// The core's marker table is host policy, not protocol. It sits in the same
/// crate the client surface is read from, so without this the generated client
/// would publish `Marker`, `MarkerClass` and `Severity` as wire types.
#[test]
fn the_marker_table_stays_out_of_the_client_surface() {
	let candidate = ItemCandidate {
		item_id: "1".to_string(),
		path: vec!["truapi".to_string(), "markers".to_string(), "Severity".to_string()],
		kind: "enum".to_string(),
	};
	assert!(should_skip_type_candidate("Severity", &candidate));
}

#[test]
fn a_protocol_type_is_still_emitted() {
	let candidate = ItemCandidate {
		item_id: "2".to_string(),
		path: vec!["truapi".to_string(), "v02".to_string(), "Severity".to_string()],
		kind: "enum".to_string(),
	};
	assert!(!should_skip_type_candidate("Severity", &candidate));
}
