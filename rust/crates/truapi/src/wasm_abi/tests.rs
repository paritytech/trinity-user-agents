use super::EventKind;

#[test]
fn event_tags_identify_responses_items_and_subscription_ends() {
	assert_eq!(
		[0, 1, 2, 3, u32::MAX].map(|tag| EventKind::try_from(tag).map_err(|error| error.input)),
		[Ok(EventKind::Response), Ok(EventKind::Item), Ok(EventKind::End), Err(3), Err(u32::MAX)]
	);
}
