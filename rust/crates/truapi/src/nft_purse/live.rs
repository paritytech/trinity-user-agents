//! NFT payments over the live People chain. Ignored by default; run with
//! `cargo test -p truapi --lib nft_purse::live -- --ignored`.

use std::sync::Arc;

use futures::StreamExt;
use parity_scale_codec::Encode;
use truapi::v01;

use super::{TEST_STATIC_SECRET, test_purses_over};
use crate::{
	host_logic::{
		statement_store::{StatementField, sign_statement_fields},
		x25519_seal::{SealedBox, X25519PrivateKey},
	},
	platform::ProductId,
	runtime::{LIVE_WAIT, LivePeopleStatements},
	store::open_temporary_core_db,
};

/// The statement a sender publishes to put `cheque` under `topic`: the
/// envelope sealed to the static receive key, signed by `signer`.
fn signed_inbox_statement(
	signer: &schnorrkel::Keypair,
	topic: v01::StatementTopic,
	cheque: &v01::NftPurseCheque,
) -> Vec<u8> {
	let plaintext = (0u8, topic, cheque).encode();
	let static_key = X25519PrivateKey::from(TEST_STATIC_SECRET).public_key();
	let sealed = SealedBox::seal(static_key, &plaintext).unwrap();
	let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
	let fields = vec![
		StatementField::Expiry((now.as_secs() + 3600) << 32),
		StatementField::Channel(rand::random()),
		StatementField::Topic1(topic.0),
		StatementField::Data(sealed.to_bytes()),
	];
	let signed = sign_statement_fields(signer.secret.to_bytes(), signer.public.to_bytes(), fields);
	signed.unwrap().encode()
}

/// A cheque a real sender publishes on the live People chain reaches a product
/// listening to its inbox. Needs a person in a People Lite ring:
/// `TRUAPI_LIVE_PERSON_MNEMONIC` names one, and the test skips without it. The
/// first run in a period takes one of the person's statement-store slots;
/// later runs reuse it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Paseo People Next chain and a person's mnemonic"]
async fn a_payment_inbox_receives_a_cheque_published_on_the_live_chain() {
	let Ok(mnemonic) = std::env::var("TRUAPI_LIVE_PERSON_MNEMONIC") else {
		eprintln!("skipped: TRUAPI_LIVE_PERSON_MNEMONIC is not set");
		return;
	};
	let entropy = bip39::Mnemonic::parse(mnemonic.trim()).unwrap().to_entropy();
	let statements = Arc::new(LivePeopleStatements::connect().await);
	let signer = statements.statement_signer_with_allowance(&entropy).await;
	let (_dir, db) = open_temporary_core_db();
	let purses = test_purses_over(db, statements.clone());
	let product = ProductId::for_test("tickets.dot");
	let inbox = purses.start_payment_inbox_listen(rand::random(), &product, rand::random());
	let mut listen = inbox.await.unwrap();
	let backlog = tokio::time::timeout(LIVE_WAIT, listen.arrivals.next()).await;
	assert_eq!(backlog.unwrap().map(Result::unwrap), Some(vec![]));
	let cheque = v01::NftPurseCheque {
		version: 0,
		id: v01::NftPurseChequeId(rand::random()),
		recipient: v01::NftPurseChequeRecipient::Static,
		items: vec![v01::NftPurseItemKey(rand::random())],
		encrypted_secrets: vec![0x55; 4],
	};
	let statement = signed_inbox_statement(&signer, listen.statement_topic, &cheque);

	statements.publish(statement).await.unwrap();

	let arrived = tokio::time::timeout(LIVE_WAIT, listen.arrivals.next()).await;
	assert_eq!(arrived.unwrap().map(Result::unwrap), Some(vec![cheque]));
}
