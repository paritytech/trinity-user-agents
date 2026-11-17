use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		PrivacyPolicy, PurseId,
		ledger::{AssetRecord, Mark, ScanProgress},
	},
	store::{Db, open_memory_core_db},
};

/// The installation the app allocated under.
const APP: InstallationId = InstallationId([5; 32]);

fn handed(outstanding: Vec<OutstandingPayment>, incoming: Vec<IncomingPayment>) -> Handover {
	Handover { installations: vec![AppInstallation { id: APP, next_coin: 8, next_entry: 2 }], outstanding, incoming }
}

/// A fresh store that recorded `handover` with its installation.
fn recorded(handover: &Handover) -> Db {
	let db = open_memory_core_db();
	let handover = handover.clone();
	block_on(db.write(move |tx| {
		ledger::mint_installation(tx, InstallationId([9; 32]), &PrivacyPolicy::fastest(), None, 0)?.unwrap();
		record(tx, &handover)
	}))
	.unwrap();
	db
}

/// The app's installations are known at once, so no reconstruction can
/// miss them, and each sent payment has its export waiting for its coins.
#[test]
fn a_recorded_hand_over_names_the_apps_installations_and_sent_payments() {
	let payment = OutstandingPayment { id: [8; 32], accounts: vec![[1; 32]] };
	let db = recorded(&handed(vec![payment], Vec::new()));
	let (handover, view) = block_on(db.read(|conn| Ok((read(conn)?, ledger::snapshot(conn)?)))).unwrap();
	let export = view.operation(&app_key([8; 32])).map(|export| (export.kind, export.exported.clone()));
	assert_eq!(
		(handover.map(|handover| handover.marks(APP)), view.installations().iter().any(|(id, _)| *id == APP), export),
		(Some(Some((8, 2))), true, Some((OpKind::Export, Vec::new())))
	);
}

/// The scan of every handed-over installation is what moves the wallet, so
/// the hand-over is done exactly when each has been scanned once.
#[test]
fn the_progress_follows_the_first_scan_of_every_handed_over_installation() {
	assert_eq!(recorded_view(&open_memory_core_db()).hand_over_progress(), HandOverProgress::None);
	let db = recorded(&handed(Vec::new(), Vec::new()));
	assert_eq!(recorded_view(&db).hand_over_progress(), HandOverProgress::Scanning);
	let scanned = ScanProgress { first_scan_done: true, resume: Vec::new() };
	block_on(db.write(move |tx| ledger::record_scan(tx, APP, &scanned))).unwrap();
	assert_eq!(recorded_view(&db).hand_over_progress(), HandOverProgress::Done);
}

fn recorded_view(db: &Db) -> crate::coinage::Snapshot {
	block_on(db.read(ledger::snapshot)).unwrap()
}

/// The payee holds the secrets of a sent coin, so once a scan adopts it the
/// wallet marks it handed out and counts it in the payment's export.
#[test]
fn an_adopted_coin_of_a_sent_payment_is_marked_and_joins_its_export() {
	let db = recorded(&handed(vec![OutstandingPayment { id: [8; 32], accounts: vec![[1; 32]] }], Vec::new()));
	let coin = Asset::Coin { purse: PurseId::MAIN, installation: APP, index: 3 };
	let exponent = Exponent::new(4).unwrap();
	block_on(db.write(move |tx| {
		ledger::adopt(tx, &[AssetRecord { asset: coin, exponent }])?;
		adopt_disclosed(tx, [8; 32], coin, exponent)
	}))
	.unwrap();
	let view = recorded_view(&db);
	let export = view.operation(&app_key([8; 32])).map(|export| export.exported.clone());
	assert_eq!((view.mark(&coin), export), (Some(Mark::Committed), Some(vec![(coin, exponent)])));
}

/// Secrets are kept no longer than the claim needs them.
#[test]
fn a_received_payment_is_forgotten_once_its_claim_started() {
	let payment = |id| IncomingPayment { id: [id; 32], claim_by: 10, coins: vec![CoinSecret([id; 64])] };
	let db = recorded(&handed(Vec::new(), vec![payment(6), payment(7)]));
	block_on(db.write(|tx| forget_incoming(tx, [6; 32]))).unwrap();
	let incoming = block_on(db.read(read)).unwrap().map(|handover| handover.incoming);
	assert_eq!(incoming, Some(vec![payment(7)]));
}
