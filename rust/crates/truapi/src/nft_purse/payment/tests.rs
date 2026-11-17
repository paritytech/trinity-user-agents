use std::sync::Arc;

use futures::executor::{block_on, block_on_stream};
use tempfile::TempDir;
use truapi::latest::{NftPurseItemKey, X25519PublicKey};

use super::*;
use crate::{
	host_logic::statement_store::StatementField,
	nft_purse::{ScriptedStatements, TEST_STATIC_SECRET, test_purses_watching},
	store::open_temporary_core_db,
};

const PRODUCT: &str = "tickets.dot";

/// An envelope sealed outside this crate, with Node's X25519, HKDF-SHA256
/// and ChaCha20-Poly1305: receive secret `[0x11; 32]`, ephemeral secret
/// `[0x22; 32]`, nonce `[0x33; 12]`, one item `[0x44; 32]` and secrets
/// `[0x55; 4]`.
const ENVELOPE_VECTOR: &str = "0faa684ed28867b97f4a6a2dee5df8ce974e76b7018e3f22a1c4cf2678570f203333333333333333333333334cb836e34bf81154c807683d9d9280ec9702d598557cc0d73aa37034581636ca1b36daf392fa3f92cef63e0dbd65c6d8718243ad3340a358595f43f2baf241670d0bbce3f55b172a14fa2016685b68961e3d128c5dbb9ce50d3bfc541539e456b1f8fc9f7da078fc4795269a59dc565d73499ae26431f9d7d975309103b40486c86a6906a989e8035e9ad6bdb499479997a34ef4b1dbdf3178";

/// The receive key of secret `[0x11; 32]`.
const VECTOR_RECEIVE_KEY: &str = "7b4e909bbe7ffe44c465a220037d608ee35897d31ef972f07f74892cb0f73f13";

fn product() -> ProductId {
	ProductId::for_test(PRODUCT)
}

/// Purse calls with a purse and a receive key into it made by
/// [`PRODUCT`], the statement store they watch, and the directory that
/// keeps their database alive.
struct Listening {
	_dir: TempDir,
	purses: NftPurses,
	statements: Arc<ScriptedStatements>,
	key: NftPurseReceiveKey,
}

fn listening_purses() -> Listening {
	let (dir, db) = open_temporary_core_db();
	let statements = Arc::new(ScriptedStatements::default());
	let purses = test_purses_watching(db, statements.clone());
	let purse = block_on(purses.create_purse(&product(), "Tickets".to_owned())).unwrap();
	let key = block_on(purses.create_receive_key(&product(), purse)).unwrap();
	Listening { _dir: dir, purses, statements, key }
}

/// A cheque for `key` that decodes and matches, though its secrets are not real.
fn cheque_for(key: NftPurseReceiveKey, item: u8) -> NftPurseCheque {
	NftPurseCheque {
		version: 0,
		id: cheque_id_for(&NftPurseChequeDestination::Ephemeral(key)),
		recipient: NftPurseChequeRecipient::Ephemeral(key),
		items: vec![NftPurseItemKey([item; 32])],
		encrypted_secrets: vec![0x55; 4],
	}
}

/// The statement a sender publishes: `plaintext` sealed to `key`.
fn sealed_statement(key: NftPurseReceiveKey, plaintext: &[u8]) -> Vec<u8> {
	let sealed = SealedBox::seal(X25519PublicKey(key.0), plaintext).unwrap();
	vec![StatementField::Data(sealed.to_bytes())].encode()
}

/// The statement a sender publishes for `cheque` under `topic`.
fn payment_statement(
	key: NftPurseReceiveKey,
	topic: StatementTopic,
	cheque: NftPurseCheque,
) -> Vec<u8> {
	let contents = EnvelopeContents { version: ENVELOPE_VERSION, topic, cheque };
	sealed_statement(key, &contents.encode())
}

/// Everything `listen` emits until it ends, with errors as their messages.
fn collected_arrivals(
	listen: PaymentListen<NftPurseCheque>,
) -> Vec<Result<NftPurseCheque, String>> {
	let arrivals = block_on_stream(listen.arrivals);
	arrivals.map(|arrival| arrival.map_err(|error| error.to_string())).collect()
}

/// Senders outside this crate derive the same statement topic.
#[test]
fn the_one_time_topic_matches_its_vector() {
	let topic = one_time_payment_topic(&NftPurseReceiveKey([1; 32]));

	let expected = "5772c8ad087d691870c3252f9b4d99b9d2e845755cda3bc015755eacfc741488";
	assert_eq!(hex::encode(topic.0), expected);
}

/// Pins the envelope format other senders implement.
#[test]
fn an_envelope_sealed_elsewhere_opens_into_its_cheque() {
	let key_bytes = hex::decode(VECTOR_RECEIVE_KEY).unwrap();
	let key = NftPurseReceiveKey(key_bytes.try_into().unwrap());
	let data = hex::decode(ENVELOPE_VECTOR).unwrap();
	let statement = vec![StatementField::Data(data)].encode();
	let expected = ExpectedCheque {
		topic: one_time_payment_topic(&key),
		recipient: NftPurseChequeRecipient::Ephemeral(key),
		id: Some(cheque_id_for(&NftPurseChequeDestination::Ephemeral(key))),
	};

	let cheque = accepted_cheque(&statement, &X25519PrivateKey::from([0x11; 32]), &expected);

	assert_eq!(cheque, Some(cheque_for(key, 0x44)));
}

/// Junk on the topic must not end a listen that waits for one cheque.
#[test]
fn a_one_time_listen_skips_what_is_not_its_cheque_and_ends_after_the_first() {
	let Listening { _dir, purses, statements, key } = listening_purses();
	let topic = one_time_payment_topic(&key);
	let envelope =
		|version, item| EnvelopeContents { version, topic, cheque: cheque_for(key, item) }.encode();
	let trailing = [envelope(ENVELOPE_VERSION, 3), vec![0]].concat();
	let recipient = |recipient| NftPurseCheque { recipient, ..cheque_for(key, 5) };
	let other_key = NftPurseChequeRecipient::Ephemeral(NftPurseReceiveKey([9; 32]));
	let wrong_id = NftPurseCheque { id: NftPurseChequeId([8; 32]), ..cheque_for(key, 7) };
	let good = cheque_for(key, 1);
	let batch = vec![
		vec![1, 2, 3],
		sealed_statement(key, b"not an envelope"),
		sealed_statement(key, &envelope(1, 2)),
		sealed_statement(key, &trailing),
		payment_statement(key, StatementTopic([7; 32]), cheque_for(key, 4)),
		payment_statement(key, topic, recipient(NftPurseChequeRecipient::Static)),
		payment_statement(key, topic, recipient(other_key)),
		payment_statement(key, topic, wrong_id),
		payment_statement(key, topic, good.clone()),
		payment_statement(key, topic, cheque_for(key, 2)),
	];
	*statements.batches.lock().unwrap() = vec![vec![], batch];

	let listen = block_on(purses.start_one_time_payment_listen(&product(), key)).unwrap();

	assert_eq!(collected_arrivals(listen), vec![Ok(good)]);
}

/// Completion means a cheque arrived, so a watch that ends first is an
/// error.
#[test]
fn a_one_time_listen_whose_watch_ends_first_ends_with_an_error() {
	let Listening { _dir, purses, statements, key } = listening_purses();
	*statements.batches.lock().unwrap() = vec![vec![]];

	let listen = block_on(purses.start_one_time_payment_listen(&product(), key)).unwrap();

	assert_eq!(collected_arrivals(listen), vec![Err(PaymentListenError::WatchEnded.to_string())]);
}

/// Another product's receive key stays invisible, and nothing is watched.
#[test]
fn a_one_time_listen_on_another_products_key_does_not_start() {
	let Listening { _dir, purses, statements, key } = listening_purses();
	let other = ProductId::for_test("other.dot");

	let started = block_on(purses.start_one_time_payment_listen(&other, key));

	assert!(matches!(started, Err(PaymentListenError::UnknownReceiveKey(found)) if found == key));
	assert!(statements.watched.lock().unwrap().is_empty());
}

/// Senders outside this crate derive the same inbox, and a product's id is
/// part of it.
#[test]
fn the_inbox_topic_matches_its_vector() {
	let topic = payment_inbox_topic([2; 32], &product(), [3; 32]);

	let expected = "d1b6e489cb8d74220dd9da720267e2298565fa869ec305707ad7e7ce1d21220a";
	assert_eq!(hex::encode(topic.0), expected);
}

/// The first batch says the inbox caught up even when it is empty, later
/// empty batches say nothing, and only `Static` cheques belong in an inbox.
#[test]
fn an_inbox_listen_emits_the_backlog_then_each_batch_with_cheques() {
	let Listening { _dir, purses, statements, key } = listening_purses();
	let domain = [3; 32];
	let topic = payment_inbox_topic([2; 32], &product(), domain);
	let static_key = NftPurseReceiveKey(X25519PrivateKey::from(TEST_STATIC_SECRET).public_key().0);
	let static_cheque = |item| NftPurseCheque {
		recipient: NftPurseChequeRecipient::Static,
		..cheque_for(key, item)
	};
	let to_inbox = |cheque| payment_statement(static_key, topic, cheque);
	*statements.batches.lock().unwrap() = vec![
		vec![],
		vec![to_inbox(static_cheque(1)), to_inbox(cheque_for(key, 2))],
		vec![to_inbox(cheque_for(key, 3))],
		vec![to_inbox(static_cheque(4))],
	];

	let listen = block_on(purses.start_payment_inbox_listen([2; 32], &product(), domain)).unwrap();

	let arrivals =
		block_on_stream(listen.arrivals).map(|arrival| arrival.map_err(|error| error.to_string()));
	let expected = vec![
		Ok(vec![]),
		Ok(vec![static_cheque(1)]),
		Ok(vec![static_cheque(4)]),
		Err(PaymentListenError::WatchEnded.to_string()),
	];
	assert_eq!(arrivals.collect::<Vec<_>>(), expected);
	assert_eq!(listen.statement_topic, topic);
}
