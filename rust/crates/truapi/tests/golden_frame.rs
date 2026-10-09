//! Binary golden-frame regression test.
//!
//! `tests/snapshots/golden-account-get.bin` holds the raw bytes of an
//! `account_get_account_request` frame. The tests assert both halves of the
//! envelope: the transport framing (`requestId`, the `(trait, method)`
//! discriminant pair, and `messageType`) and the *typed decode of the
//! payload*.
//!
//! Both halves are needed. The payload is inlined as opaque bytes, so a
//! `ProtocolMessage`-only assertion is satisfied by a payload of any length,
//! and round-tripping the in-memory shape cancels a symmetric layout change
//! out. Only decoding the payload into its current type reads what the bytes
//! actually say.
//!
//! The frame encodes:
//!   requestId = "p:1"
//!   messageType = Request
//!   payload   = account_get_account,
//!               inner = HostAccountGetRequest::V1(ProductAccountId {
//!                   dot_ns_identifier: "foo",
//!                   derivation_index: DerivationIndex::Index(0),
//!               })
//!
//! On the wire (17 bytes):
//!   [0c 70 3a 31]                      requestId = compact-len(3) + "p:1"
//!   [02]                               trait discriminant 2 = account
//!   [01]                               method discriminant 1 = get_account
//!   [00]                               messageType: Request
//!   [00]                               request wrapper version V1
//!   [0c 66 6f 6f]                      compact-len(3) + "foo"
//!   [00]                               DerivationIndex variant Index
//!   [00 00 00 00]                      u32 = 0
//!
//! If this test fails after a wire-protocol change, regenerate the file
//! deliberately, re-check the change against the wire table, and treat a
//! payload layout change as breaking for every product built against an
//! older `@parity/truapi`.

use parity_scale_codec::{Decode, Encode};
use truapi::{
	frame::{MESSAGE_TYPE_REQUEST, Payload, ProtocolMessage},
	generated::wire_table,
	v01,
	versioned::account::HostAccountGetRequest,
};

const GOLDEN: &[u8] = include_bytes!("snapshots/golden-account-get.bin");

/// Payload byte count of the golden frame: the request wrapper's own version
/// tag, a compact-length-prefixed 3-byte identifier, one `DerivationIndex`
/// variant byte, and a `u32`. Spelled out term by term rather than measured
/// from the codec, so a layout change has to move this number by hand.
const GOLDEN_PAYLOAD_LEN: usize = 1 + 1 + 3 + 1 + 4;

fn expected_request() -> HostAccountGetRequest {
	HostAccountGetRequest::V1(v01::HostAccountGetRequest {
		product_account_id: v01::ProductAccountId {
			dot_ns_identifier: "foo".to_string(),
			derivation_index: v01::DerivationIndex::Index(0),
		},
	})
}

#[test]
fn golden_account_get_frame_decodes_to_expected_message() {
	let decoded = ProtocolMessage::decode(&mut &GOLDEN[..])
		.expect("golden frame must decode with the current wire codec");

	let expected = ProtocolMessage {
		request_id: "p:1".to_string(),
		payload: Payload {
			trait_id: wire_table::ACCOUNT_GET_ACCOUNT.trait_id,
			method_id: wire_table::ACCOUNT_GET_ACCOUNT.method_id,
			message_type: MESSAGE_TYPE_REQUEST,
			value: expected_request().encode(),
		},
	};
	assert_eq!(decoded, expected);
}

#[test]
fn golden_account_get_payload_decodes_as_the_typed_request() {
	let decoded = ProtocolMessage::decode(&mut &GOLDEN[..]).expect("decode");
	assert_eq!(decoded.payload.message_type, MESSAGE_TYPE_REQUEST);
	assert_eq!(
		decoded.payload.value.len(),
		GOLDEN_PAYLOAD_LEN,
		"account_get_account request payload changed length; every product \
         built against an older @parity/truapi now fails to decode"
	);

	let request = HostAccountGetRequest::decode(&mut &decoded.payload.value[..])
		.expect("golden payload must decode as the typed request wrapper");
	assert_eq!(request, expected_request());
}

#[test]
fn golden_account_get_frame_round_trips() {
	// Encoding the in-memory shape must reproduce the on-disk bytes exactly.
	let decoded = ProtocolMessage::decode(&mut &GOLDEN[..]).expect("decode");
	assert_eq!(decoded.encode(), GOLDEN);
}
