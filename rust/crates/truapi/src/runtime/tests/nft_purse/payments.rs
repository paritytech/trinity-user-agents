//! Listening for payments through the product-facing trait.

use futures::{StreamExt, executor::block_on};
use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::versioned::nft_purse::{
	HostNftPurseListenOneTimePaymentError, HostNftPurseListenOneTimePaymentItem,
	HostNftPurseListenPaymentInboxError, HostNftPurseListenPaymentInboxItem,
	HostNftPurseListenPaymentInboxRequest,
};

use super::{cheques::create_receive_key, operations::product_purses, *};
use crate::{
	host_logic::{
		session::SessionInfo,
		statement_store::StatementField,
		x25519_seal::{SealedBox, X25519PrivateKey},
	},
	test_support::{session_info, statement_page_frame, subscribe_ack_frame},
};

type OneTimePayments = Subscription<
	HostNftPurseListenOneTimePaymentItem,
	CallError<HostNftPurseListenOneTimePaymentError>,
>;

fn listen_one_time(host: &ProductRuntimeHost, key: v01::NftPurseReceiveKey) -> OneTimePayments {
	let request = v01::HostNftPurseListenOneTimePaymentRequest { key };
	let request = HostNftPurseListenOneTimePaymentRequest::V1(request);
	block_on(host.listen_one_time_payment(&CallContext::default(), request))
}

/// The JSON-RPC methods `platform` was sent, in order.
fn sent_methods(platform: &StubPlatform) -> Vec<String> {
	let sent = platform.sent_rpc.lock().unwrap();
	let method = |request: &String| {
		let request: serde_json::Value = serde_json::from_str(request).unwrap();
		request["method"].as_str().unwrap_or_default().to_owned()
	};
	sent.iter().map(method).collect()
}

/// The topic of the first statement subscription `platform` was sent.
fn subscribed_topic(platform: &StubPlatform) -> v01::StatementTopic {
	let sent = platform.sent_rpc.lock().unwrap();
	let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
	let topic = request["params"][0]["matchAll"][0].as_str().unwrap();
	let topic = hex::decode(topic.trim_start_matches("0x")).unwrap();
	v01::StatementTopic(topic.try_into().unwrap())
}

/// A platform whose statement subscription is acknowledged, with nothing
/// stored under its topic.
fn platform_with_empty_topic() -> Arc<StubPlatform> {
	Arc::new(StubPlatform {
		rpc_responses: vec![
			subscribe_ack_frame("truapi:1", "payments"),
			statement_page_frame("payments", vec![], Some(0)),
		],
		..Default::default()
	})
}

/// The product shares the channel with the sender, so it comes first, and it
/// is the topic the host watches.
#[test]
fn a_one_time_listen_emits_the_watched_channel_first() {
	let platform = platform_with_empty_topic();
	let purses = product_purses(platform.clone());
	let key = create_receive_key(&purses.host, TICKETS).unwrap();

	let first = block_on(listen_one_time(&purses.host, key).next());

	let sss_topic = subscribed_topic(&platform);
	let channel = v01::NftPursePaymentTransmissionChannel::Standard { sss_topic };
	let expected = HostNftPurseListenOneTimePaymentItem::V1(
		v01::HostNftPurseListenOneTimePaymentItem::Channel(channel),
	);
	assert_eq!(first, Some(Ok(expected)));
}

/// A receive key this host never made reveals nothing and opens no
/// subscription.
#[test]
fn a_one_time_listen_on_an_unknown_key_is_refused() {
	let platform = platform_with_empty_topic();
	let purses = product_purses(platform.clone());

	let first = block_on(listen_one_time(&purses.host, v01::NftPurseReceiveKey([1; 32])).next());

	let refused = HostNftPurseListenOneTimePaymentError::V1(
		v01::NftPurseListenOneTimePaymentError::ReceiveKeyNotFound,
	);
	assert_eq!(first, Some(Err(CallError::Domain(refused))));
	assert!(sent_methods(&platform).is_empty());
}

#[test]
fn a_one_time_listen_needs_a_paired_session() {
	let (_dir, host) = nft_host(platform_with_empty_topic(), false);

	let first = block_on(listen_one_time(&host, v01::NftPurseReceiveKey([1; 32])).next());

	assert_eq!(first, Some(Err(CallError::Denied)));
}

/// A listen that never watched must not hand out a channel nobody watches.
#[test]
fn a_one_time_listen_that_cannot_connect_emits_no_channel() {
	let platform = Arc::new(StubPlatform {
		chain_connect_error: Some("the People chain is unreachable"),
		..Default::default()
	});
	let purses = product_purses(platform);
	let key = create_receive_key(&purses.host, TICKETS).unwrap();

	let mut payments = listen_one_time(&purses.host, key);

	let first = block_on(payments.next());
	assert!(matches!(
		first,
		Some(Err(CallError::Domain(HostNftPurseListenOneTimePaymentError::V1(
			v01::NftPurseListenOneTimePaymentError::Other(_)
		))))
	));
	assert_eq!(block_on(payments.next()), None);
}

/// No subscription outlives the call that opened it.
#[test]
fn dropping_a_one_time_listen_unsubscribes() {
	let platform = platform_with_empty_topic();
	let purses = product_purses(platform.clone());
	let key = create_receive_key(&purses.host, TICKETS).unwrap();
	let mut payments = listen_one_time(&purses.host, key);
	block_on(payments.next());

	drop(payments);

	let methods = sent_methods(&platform);
	assert_eq!(methods.last().map(String::as_str), Some("statement_unsubscribeStatement"));
}

type InboxPayments = Subscription<
	HostNftPurseListenPaymentInboxItem,
	CallError<HostNftPurseListenPaymentInboxError>,
>;

const INBOX_DOMAIN: [u8; 32] = [3; 32];

fn listen_inbox(host: &ProductRuntimeHost) -> InboxPayments {
	let request = v01::HostNftPurseListenPaymentInboxRequest { inbox_domain: INBOX_DOMAIN };
	let request = HostNftPurseListenPaymentInboxRequest::V1(request);
	block_on(host.listen_payment_inbox(&CallContext::default(), request))
}

/// The inbox topic of [`PRODUCT`] and [`INBOX_DOMAIN`] for the test session,
/// derived the way a sender outside the host derives it.
fn inbox_topic() -> v01::StatementTopic {
	let account = session_info().identity_account_id.unwrap();
	let product = blake2_256(PRODUCT.as_bytes());
	let preimage =
		[b"nft-purse/payment-inbox".as_slice(), &account, &product, &INBOX_DOMAIN].concat();
	v01::StatementTopic(blake2_256(&preimage))
}

/// The statement a sender publishes to put `cheque` in the inbox: the
/// envelope sealed to the static receive key.
fn inbox_statement(cheque: &v01::NftPurseCheque) -> Vec<u8> {
	let plaintext = (0u8, inbox_topic(), cheque).encode();
	let static_key = X25519PrivateKey::from(TEST_STATIC_SECRET).public_key();
	let sealed = SealedBox::seal(static_key, &plaintext).unwrap();
	vec![StatementField::Data(sealed.to_bytes())].encode()
}

fn inbox_item(item: v01::HostNftPurseListenPaymentInboxItem) -> HostNftPurseListenPaymentInboxItem {
	HostNftPurseListenPaymentInboxItem::V1(item)
}

/// A cheque a sender built and published reaches the product, after the
/// channel it was published on.
#[test]
fn an_inbox_listen_delivers_the_cheques_stored_in_the_inbox() {
	let cheque = v01::NftPurseCheque {
		version: 0,
		id: v01::NftPurseChequeId([6; 32]),
		recipient: v01::NftPurseChequeRecipient::Static,
		items: vec![v01::NftPurseItemKey([7; 32])],
		encrypted_secrets: vec![8; 4],
	};
	let platform = Arc::new(StubPlatform {
		rpc_responses: vec![
			subscribe_ack_frame("truapi:1", "inbox"),
			statement_page_frame("inbox", vec![inbox_statement(&cheque)], Some(0)),
		],
		..Default::default()
	});
	let (_dir, host) = nft_host(platform, true);

	let items = block_on(listen_inbox(&host).take(2).collect::<Vec<_>>());

	let channel = v01::NftPursePaymentTransmissionChannel::Standard { sss_topic: inbox_topic() };
	let expected = vec![
		Ok(inbox_item(v01::HostNftPurseListenPaymentInboxItem::Channel(channel))),
		Ok(inbox_item(v01::HostNftPurseListenPaymentInboxItem::Cheques(vec![cheque]))),
	];
	assert_eq!(items, expected);
}

/// Without the account a sender names the inbox by, there is no inbox to
/// watch.
#[test]
fn an_inbox_listen_without_an_identity_account_is_refused() {
	let platform = platform_with_empty_topic();
	let (_dir, host) = nft_host(platform.clone(), false);
	install_pairing_session(&host, SessionInfo { identity_account_id: None, ..session_info() });

	let first = block_on(listen_inbox(&host).next());

	assert!(matches!(
		first,
		Some(Err(CallError::Domain(HostNftPurseListenPaymentInboxError::V1(
			v01::NftPurseListenPaymentInboxError::Other(_)
		))))
	));
	assert!(sent_methods(&platform).is_empty());
}
