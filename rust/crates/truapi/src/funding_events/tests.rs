use super::*;

fn event(marker: Marker, tags: &[&'static str], measures: &[&'static str]) -> FundingEvent {
	FundingEvent {
		marker,
		session_ref: "ref".to_string(),
		tags: tags.iter().map(|key| (*key, "x".to_string())).collect(),
		measures: measures.iter().map(|key| (*key, 1)).collect(),
	}
}

#[test]
fn every_event_is_an_exported_funding_marker() {
	for spec in FUNDING_EVENTS {
		let target = spec.marker.target;
		assert!(target.starts_with("truapi.funding."), "{target} is not a funding marker");
		assert_eq!(markers::class_of(target), Some(spec.marker.class), "{target} is unlisted");
		assert!(
			matches!(spec.marker.class, markers::MarkerClass::Export { .. }),
			"{target} stays on the device"
		);
		assert_eq!(spec_of(target).map(|found| found.marker), Some(spec.marker));
	}
	let errors: Vec<&str> = FUNDING_EVENTS
		.iter()
		.filter(|spec| {
			spec.marker.class == markers::MarkerClass::Export { severity: markers::Severity::Error }
		})
		.map(|spec| spec.marker.target)
		.collect();
	assert_eq!(errors, ["truapi.funding.ended_unexpectedly"]);
}

#[test]
fn an_unknown_marker_is_refused() {
	assert_eq!(
		check(&event(markers::FUNDING_SWEEP_FAILED, &[], &[])),
		Err(Refused::UnknownMarker("truapi.funding.sweep_failed"))
	);
	assert!(check(&event(markers::CORE_STARTED, &[], &[])).is_err());
}

#[test]
fn a_key_its_entry_does_not_name_is_refused() {
	assert_eq!(
		check(&event(markers::FUNDING_OVERLAY_OPENED, &["intent"], &[])),
		Err(Refused::UnknownKey { marker: "truapi.funding.overlay_opened", key: "intent" })
	);
	assert!(check(&event(markers::FUNDING_OVERLAY_OPENED, &[], &["amount"])).is_err());
	// Another event's key is not this one's.
	assert!(check(&event(markers::FUNDING_OVERLAY_OPENED, &["step"], &[])).is_err());
}

#[test]
fn common_and_own_keys_are_accepted() {
	assert_eq!(
		check(&event(
			markers::FUNDING_ENDED,
			&["direction", "outcome", "region"],
			&["age_ms", "duration_ms"],
		)),
		Ok(())
	);
}

#[test]
fn a_detail_a_host_would_cut_is_refused() {
	let mut long = event(markers::FUNDING_OVERLAY_OPENED, &["amount_preset"], &[]);
	long.session_ref = "r".repeat(HOST_SINK_DETAIL_MAX_CHARS);
	assert_eq!(check(&long), Err(Refused::TooLong("truapi.funding.overlay_opened")));
}

// A detail is the reference, then the tags, then the measures, in order.
#[test]
fn a_detail_reads_ref_then_tags_then_measures() {
	let logged = FundingEvent {
		marker: markers::FUNDING_STEP_REACHED,
		session_ref: "abc".to_string(),
		tags: vec![("direction", "in".to_string()), ("step", "received".to_string())],
		measures: vec![("age_ms", 7), ("since_start_ms", 5)],
	};
	assert_eq!(
		logged.detail(),
		"funding.ref=abc direction=in step=received age_ms=7 since_start_ms=5"
	);
}

// An emitted fact reaches the host's sink as its marker and its detail, the
// way every exported marker does; one the table refuses never does.
#[test]
fn an_emitted_fact_reaches_the_host_sink_as_its_marker() {
	let fact = FundingEvent {
		marker: markers::FUNDING_ENDED_UNEXPECTEDLY,
		session_ref: "ref".to_string(),
		tags: vec![("outcome", "failed".to_string())],
		measures: vec![("duration_ms", 12)],
	};
	let refused = event(markers::FUNDING_OVERLAY_OPENED, &["intent"], &[]);
	let lines = crate::logging::testing::host_sink_lines(|| {
		assert_eq!(emit(&fact), Ok("funding.ref=ref outcome=failed duration_ms=12".to_string()));
		assert!(emit(&refused).is_err());
	});
	assert_eq!(
		lines,
		["truapi.funding.ended_unexpectedly funding.ref=ref outcome=failed duration_ms=12"]
	);
}
