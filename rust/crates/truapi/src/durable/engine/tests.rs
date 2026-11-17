use futures::{StreamExt, executor::block_on};

use super::*;
use crate::{
	chain::MortalExtrinsic,
	durable::testing::{FakeChain, extrinsic, pending_presigned, test_engine, without_plan},
};

fn domain() -> DomainId {
	DomainId::new("test")
}

fn request(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest {
	DurableRequest::presigned(domain(), Some(GroupId::new("op")), without_plan(extrinsics)).unwrap()
}

#[test]
fn status_reads_and_streams_follow_the_ledger() {
	let chain = FakeChain::new(130, 140);
	let (_dir, engine, _timer) = test_engine(&chain);
	let id = block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap()[0];
	let mut status = engine.observe_status(id);
	let mut group = engine.observe_group(&domain(), &GroupId::new("op"));

	assert_eq!(block_on(status.next()).unwrap().unwrap(), Some(DurableTxStatus::Pending));
	assert_eq!(block_on(group.next()).unwrap().unwrap(), vec![pending_presigned(id, 1)]);
	assert_eq!(block_on(engine.status(id)).unwrap(), Some(DurableTxStatus::Pending));
	assert_eq!(block_on(engine.status(DurableTxId(99))).unwrap(), None);
}
