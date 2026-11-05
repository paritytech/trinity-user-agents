use super::*;

#[test]
fn resource_allocation_summary_reflects_per_resource_outcomes() {
	let result = resource_allocation_outcome(&Ok(vec![SsoAllocationOutcome::Allocated(
		SsoAllocatedResource::SmartContractAllowance,
	)]));
	assert_eq!((result.outcome, result.reason), ("ok", None));

	let result = resource_allocation_outcome(&Ok(vec![SsoAllocationOutcome::Rejected]));
	assert_eq!(
		(result.outcome, result.reason.as_deref()),
		("rejected", Some("Requested resource was rejected"))
	);

	let result = resource_allocation_outcome(&Ok(vec![
		SsoAllocationOutcome::Allocated(SsoAllocatedResource::BulletinAllowance {
			slot_account_key: vec![1; 64],
		}),
		SsoAllocationOutcome::Rejected,
		SsoAllocationOutcome::NotAvailable,
	]));
	assert_eq!(
		(result.outcome, result.reason.as_deref()),
		("partial", Some("1 of 3 requested resources allocated; 1 rejected; 1 unavailable"))
	);

	let result = resource_allocation_outcome(&Ok(vec![SsoAllocationOutcome::NotAvailable]));
	assert_eq!(
		(result.outcome, result.reason.as_deref()),
		("not_available", Some("Requested resource is not available"))
	);
}

#[test]
fn mixed_unallocated_resources_report_rejection() {
	let result = resource_allocation_outcome(&Ok(vec![
		SsoAllocationOutcome::Rejected,
		SsoAllocationOutcome::NotAvailable,
	]));

	assert_eq!(
		(result.outcome, result.reason.as_deref()),
		("rejected", Some("No resources allocated; 1 rejected; 1 unavailable"))
	);
}

#[test]
fn allocation_transcript_includes_single_line_item_failures() {
	let answer = allocation_reply(
		Ok(vec![SsoAllocationOutcome::NotAvailable]),
		vec!["rpc\nfailed".to_string(), "provider\rdown".to_string()],
	)
	.finish(
		"allocation-1",
		crate::host_internal::sso_messages::v1::RemoteMessage::ResourceAllocationResponse,
	);

	assert_eq!(answer.outcome.outcome, "not_available");
	assert_eq!(
		answer.outcome.reason.as_deref(),
		Some("Requested resource is not available: rpc failed; provider down")
	);
}
