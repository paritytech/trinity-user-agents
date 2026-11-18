use futures::executor::block_on;

use super::*;
use crate::{
	coinage::ledger::snapshot,
	store::{Db, open_memory_core_db},
};

fn key(id: u8) -> OpKey {
	OpKey { scope: b"product".to_vec(), id: [id; 32] }
}

fn begin_on(db: &Db, key: OpKey, kind: OpKind, arguments: &'static [u8]) -> Result<Begun, OperationError> {
	block_on(db.write(move |tx| begin(tx, &key, kind, arguments))).unwrap()
}

fn cancel_on(db: &Db, key: OpKey) -> Result<(), OperationError> {
	block_on(db.write(move |tx| request_cancel(tx, &key))).unwrap()
}

fn stored(db: &Db, key: &OpKey) -> OperationRecord {
	block_on(db.read(snapshot)).unwrap().operation(key).cloned().unwrap()
}

#[test]
fn a_repeated_begin_joins_and_a_different_one_is_refused() {
	let db = open_memory_core_db();
	assert_eq!(begin_on(&db, key(1), OpKind::Rebalance, b"args"), Ok(Begun::New));
	assert_eq!(begin_on(&db, key(1), OpKind::Rebalance, b"args"), Ok(Begun::Existing));
	assert_eq!(begin_on(&db, key(1), OpKind::Rebalance, b"other"), Err(OperationError::KeyReused));
	assert_eq!(begin_on(&db, key(1), OpKind::Offload, b"args"), Err(OperationError::KeyReused));
	let other_scope = OpKey { scope: b"another".to_vec(), id: [1; 32] };
	assert_eq!(begin_on(&db, other_scope, OpKind::Offload, b"args"), Ok(Begun::New));
}

#[test]
fn cancellation_is_refused_for_an_export_and_the_layers_own_operations() {
	let db = open_memory_core_db();
	assert_eq!(cancel_on(&db, key(1)), Err(OperationError::UnknownOperation));
	begin_on(&db, key(1), OpKind::Offload, b"").unwrap();
	begin_on(&db, key(2), OpKind::Export, b"").unwrap();
	begin_on(&db, OpKey::own([3; 32]), OpKind::Rescue, b"").unwrap();
	assert_eq!(cancel_on(&db, key(1)), Ok(()));
	assert!(stored(&db, &key(1)).cancel_requested);
	assert_eq!(cancel_on(&db, key(2)), Err(OperationError::NotCancellable));
	assert_eq!(cancel_on(&db, OpKey::own([3; 32])), Err(OperationError::NotCancellable));
}

#[test]
fn an_offload_stores_its_wait_and_its_end() {
	let db = open_memory_core_db();
	begin_on(&db, key(1), OpKind::Offload, b"").unwrap();
	block_on(db.write(|tx| record_offload_wait(tx, &key(1), Some(500)))).unwrap();
	assert_eq!(stored(&db, &key(1)).offload_wait, Some(Some(500)));
	block_on(db.write(|tx| record_offload_wait(tx, &key(1), None))).unwrap();
	assert_eq!(stored(&db, &key(1)).offload_wait, Some(None));
	block_on(db.write(|tx| clear_offload_wait(tx, &key(1)))).unwrap();
	assert_eq!(stored(&db, &key(1)).offload_wait, None);
	block_on(db.write(|tx| record_end(tx, &key(1), OperationEnd::Done))).unwrap();
	assert_eq!(stored(&db, &key(1)).end, Some(OperationEnd::Done));
}

#[test]
fn a_release_is_recorded() {
	let db = open_memory_core_db();
	begin_on(&db, key(4), OpKind::Export, b"").unwrap();
	block_on(db.write(|tx| record_release(tx, &key(4)))).unwrap();
	assert!(stored(&db, &key(4)).released);
}

#[test]
fn the_group_is_stable_and_separates_scopes() {
	assert_eq!(key(1).group(), key(1).group());
	assert_ne!(key(1).group(), OpKey::own([1; 32]).group());
	assert!(key(1).group().as_str().starts_with("coinage-op:"));
}
