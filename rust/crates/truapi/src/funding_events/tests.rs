use super::*;
use crate::platform::{FundingEventLevel, FundingEventMeasure, FundingEventTag};

fn event(name: &str, tags: &[&str], measures: &[&str]) -> FundingEvent {
	FundingEvent {
		name: name.to_string(),
		level: FundingEventLevel::Info,
		session_ref: "ref".to_string(),
		tags: tags
			.iter()
			.map(|key| FundingEventTag { key: key.to_string(), value: "x".to_string() })
			.collect(),
		measures: measures
			.iter()
			.map(|key| FundingEventMeasure { key: key.to_string(), value: 1 })
			.collect(),
	}
}

#[test]
fn the_table_and_the_lookup_agree() {
	for spec in FUNDING_EVENTS {
		assert_eq!(spec_of(spec.name), Some(spec), "{} is unreachable", spec.name);
		assert!(
			spec.name.starts_with("truapi.funding."),
			"{} is not a core funding fact",
			spec.name
		);
	}
}

#[test]
fn an_unknown_name_is_refused() {
	assert_eq!(
		check(&event("truapi.funding.not_registered", &[], &[])),
		Err(Refused::UnknownName("truapi.funding.not_registered".to_string()))
	);
	assert!(check(&event("host.funding.entry_opened", &[], &[])).is_err());
}

#[test]
fn a_key_its_entry_does_not_name_is_refused() {
	assert_eq!(
		check(&event(OVERLAY_OPENED.name, &["funding.intent"], &[])),
		Err(Refused::UnknownKey {
			name: OVERLAY_OPENED.name.to_string(),
			key: "funding.intent".to_string(),
		})
	);
	assert!(check(&event(OVERLAY_OPENED.name, &[], &["funding.amount"])).is_err());
	// Another event's key is not this one's.
	assert!(check(&event(OVERLAY_OPENED.name, &["funding.step"], &[])).is_err());
}

#[test]
fn common_and_own_keys_are_accepted() {
	assert_eq!(
		check(&event(
			ENDED.name,
			&["funding.direction", "funding.outcome", "funding.region"],
			&["funding.age_ms", "funding.duration_ms"],
		)),
		Ok(())
	);
}
