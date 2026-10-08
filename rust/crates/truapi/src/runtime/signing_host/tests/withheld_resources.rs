//! A withheld resource is refused while the rest of the request is granted.

use super::*;
use crate::test_support::statement;
use truapi::api::StatementStore;
use truapi::versioned::statement_store::RemoteStatementStoreCreateProofAuthorizedRequest;

/// A platform that approves the allocation, so a refusal below is the
/// withholding rather than a declined confirmation.
fn granting_platform() -> Arc<StubPlatform> {
    Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        ..StubPlatform::default()
    })
}

/// Ask for `resources` and return the per-resource outcomes.
fn allocate(
    runtime: &ProductRuntimeHost,
    resources: Vec<v01::AllocatableResource>,
) -> Vec<v01::AllocationOutcome> {
    let response = futures::executor::block_on(ResourceAllocation::request(
        runtime,
        &CallContext::default(),
        HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
            resources,
        }),
    ))
    .expect("an approved allocation request is answered");
    let HostRequestResourceAllocationResponse::V1(response) = response;
    response.outcomes
}

#[test]
fn a_withheld_resource_is_refused_while_the_others_are_granted() {
    let (services, activation) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(true);
    activation.set_withheld_resources(vec!["AutoSigning".to_string()]);
    let runtime = product_runtime(services, activation);

    // The order is the request's, so a suite reads each resource's own answer
    // rather than one verdict for the batch.
    assert_eq!(
        allocate(
            &runtime,
            vec![
                v01::AllocatableResource::AutoSigning,
                v01::AllocatableResource::StatementStoreAllowance,
            ],
        ),
        vec![
            v01::AllocationOutcome::Rejected,
            v01::AllocationOutcome::Allocated,
        ],
    );
}

#[test]
fn withholding_nothing_leaves_every_resource_granted() {
    let (services, activation) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(true);
    let runtime = product_runtime(services, activation);

    assert_eq!(
        allocate(&runtime, vec![v01::AllocatableResource::AutoSigning]),
        vec![v01::AllocationOutcome::Allocated],
    );
}

#[test]
fn a_later_set_replaces_the_earlier_one() {
    let (services, activation) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(true);
    activation.set_withheld_resources(vec!["AutoSigning".to_string()]);
    activation.set_withheld_resources(vec!["BulletinAllowance".to_string()]);
    let runtime = product_runtime(services, activation);

    // Replacing rather than accumulating: a suite that narrows what it withholds
    // would otherwise keep refusing whatever it named first.
    assert_eq!(
        allocate(
            &runtime,
            vec![
                v01::AllocatableResource::AutoSigning,
                v01::AllocatableResource::BulletinAllowance,
            ],
        ),
        vec![
            v01::AllocationOutcome::Allocated,
            v01::AllocationOutcome::Rejected,
        ],
    );
}

/// Ask for a statement proof the way `createProofAuthorized` does.
fn proof_is_signed(runtime: &ProductRuntimeHost) -> bool {
    futures::executor::block_on(StatementStore::create_proof_authorized(
        runtime,
        &CallContext::default(),
        RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement()),
    ))
    .is_ok()
}

/// A product reaching a statement-store allowance never asks for an
/// allocation: it calls for the key, which allocates on its own. Withholding
/// that reached only the allocation answer would tell the product `Rejected`
/// and then sign for it anyway, so a suite proving its product lives without
/// the allowance would be watching the path where it has one.
#[test]
fn a_withheld_statement_store_allowance_leaves_the_proof_path_unsigned() {
    let (services, activation) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(true);
    activation.set_withheld_resources(vec!["StatementStoreAllowance".to_string()]);
    let runtime = product_runtime(services, activation);

    assert!(!proof_is_signed(&runtime));
}

/// The control the case above needs. Unchecked granting is what lets either
/// test reach the key without a chain, so without this the refusal there could
/// equally be a host that was never going to sign.
#[test]
fn a_granted_statement_store_allowance_signs_the_proof_path() {
    let (services, activation) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(true);
    let runtime = product_runtime(services, activation);

    assert!(proof_is_signed(&runtime));
}

/// The bulletin keys allocate on their own too, both the first one a preimage
/// submission asks for and the refreshed one it falls back to.
#[test]
fn a_withheld_bulletin_allowance_yields_no_key_on_either_call() {
    let (_services, activation) = signing_runtime_with_platform(granting_platform());
    futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    activation.set_grant_allowances_unchecked(true);
    activation.set_withheld_resources(vec!["BulletinAllowance".to_string()]);
    let session = activation.current_session().expect("the session just made");
    let cx = CallContext::default();

    let keys = futures::executor::block_on(async {
        [
            activation
                .bulletin_allowance_key(&cx, &session, "myapp.dot".to_string())
                .await
                .is_ok(),
            activation
                .refresh_bulletin_allowance_key(&cx, &session, "myapp.dot".to_string())
                .await
                .is_ok(),
        ]
    });

    assert_eq!(keys, [false, false]);
}
