use truapi::latest::{FundingDirection, FundingFailure};

use super::*;
use crate::host_logic::funding::FundingStage;

fn ended() -> HostFundingStatusSubscribeItem {
	HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired }
}

fn flying() -> HostFundingStatusSubscribeItem {
	HostFundingStatusSubscribeItem::InProgress { expires_at: None }
}

// An ended session is owed to the host until acknowledged, whether the core
// still holds it in full or only its summary; one in flight is not.
#[test]
fn ended_sessions_stay_owed_until_acknowledged() {
	let book = FundingSessionBook::default();
	let mut held = FundingSession::new("held".to_string(), None, FundingDirection::Out, None, 1);
	held.stage = FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms: 2 };
	let open = FundingSession::new("open".to_string(), None, FundingDirection::In, None, 1);

	book.session_changed("held", &ended(), Some(&held));
	book.session_changed("open", &flying(), Some(&open));
	book.session_changed("packed", &ended(), None);
	let owed = book.unacknowledged();
	book.acknowledged("packed");

	assert_eq!(
		(owed, book.unacknowledged()),
		(vec!["held".to_string(), "packed".to_string()], vec!["held".to_string()])
	);
}

// Divergence "Ack ended sessions": the book acknowledges only what the host
// says it showed, never on a reload.
#[test]
fn reading_the_activity_acknowledges_nothing() {
	let book = FundingSessionBook::default();
	book.session_changed("packed", &ended(), None);
	let _ = book.activity(Vec::new, Vec::new);
	assert_eq!(book.unacknowledged(), ["packed"]);
}

#[test]
fn an_overlay_closing_drops_its_state() {
	let book = FundingSessionBook::default();
	book.options_changed(
		"fs_1",
		FundingOptions {
			candidates: Vec::new(),
			countries: Vec::new(),
			limits: Default::default(),
			networks: Vec::new(),
			rails: Vec::new(),
			default_rail: truapi::latest::FundingRail::Card,
			asset: None,
			tokens: Vec::new(),
			detected_country: None,
		},
	);
	let before = book.options("fs_1").is_some();
	book.overlay_closed("fs_1");
	assert_eq!((before, book.options("fs_1")), (true, None));
}

#[test]
fn the_resume_marker_follows_whether_funding_is_in_progress() {
	let directory = tempfile::tempdir().expect("temp dir");
	write_resume_marker(directory.path(), true);
	let pending = resume_marker(directory.path()).exists();
	write_resume_marker(directory.path(), false);
	write_resume_marker(directory.path(), false);
	assert_eq!((pending, resume_marker(directory.path()).exists()), (true, false));
}
