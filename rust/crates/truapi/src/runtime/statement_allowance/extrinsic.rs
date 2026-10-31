//! Call arguments for allowance extrinsics, which the core's
//! `ExtrinsicService` assembles. They encode by name against metadata, so a
//! renamed or reshaped call fails loudly instead of encoding a wrong one.

use subxt::ext::scale_encode::EncodeAsType;

/// `Resources.set_statement_store_account` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct SetStatementStoreAccountCallArgs {
	/// Statement-store period.
	pub period: u32,
	/// Slot within the period.
	pub seq: u32,
	/// Account the allowance is for.
	pub target_account: [u8; 32],
}

/// `Resources.claim_long_term_storage` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ClaimLongTermStorageCallArgs {
	/// Long-term-storage period.
	pub period: u32,
	/// Claim counter within the period.
	pub counter: u8,
	/// Account the storage is for.
	pub account_id: [u8; 32],
}

/// `Resources.set_notification_statement_account_for_sequence` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct SetNotificationAccountCallArgs {
	/// Notification period and the slot within it. The runtime decodes the
	/// slot as one byte.
	pub reference: (u32, u8),
	/// Account the notification allowance is for.
	pub account_id: [u8; 32],
}

/// `Pgas.claim_pgas` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ClaimPgasCallArgs {
	/// PGAS slot of the day.
	pub slot_index: u32,
	/// Account credited.
	pub target: [u8; 32],
}

#[cfg(test)]
mod typed_call_tests {
	use subxt::{
		Metadata as SubxtMetadata,
		ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsFields},
	};

	use super::{super::test_fixtures, *};

	fn encode_typed(
		metadata_bytes: &[u8],
		pallet: &str,
		call: &str,
		args: &impl EncodeAsFields,
	) -> Vec<u8> {
		let metadata = SubxtMetadata::decode_from(metadata_bytes).unwrap();
		encode_call_data(pallet, call, args, &metadata, metadata.types()).unwrap()
	}

	#[test]
	fn the_statement_store_call_has_the_reference_bytes() {
		let args =
			SetStatementStoreAccountCallArgs { period: 7, seq: 3, target_account: [0x22; 32] };
		let expected =
			"3f0a07000000030000002222222222222222222222222222222222222222222222222222222222222222";

		let encoded = encode_typed(
			test_fixtures::PEOPLE_METADATA,
			"Resources",
			"set_statement_store_account",
			&args,
		);

		assert_eq!(hex::encode(encoded), expected);
	}

	#[test]
	fn the_long_term_storage_call_has_the_reference_bytes() {
		let args = ClaimLongTermStorageCallArgs { period: 9, counter: 2, account_id: [0x33; 32] };
		let expected =
			"3f0c09000000023333333333333333333333333333333333333333333333333333333333333333";

		let encoded = encode_typed(
			test_fixtures::PEOPLE_METADATA,
			"Resources",
			"claim_long_term_storage",
			&args,
		);

		assert_eq!(hex::encode(encoded), expected);
	}

	/// Only the V16 People capture declares the notification call, and the
	/// runtime decodes `seq` as one byte: a wider field would shift the account.
	#[test]
	fn the_notification_call_has_the_reference_bytes() {
		let args = SetNotificationAccountCallArgs { reference: (7, 3), account_id: [0x22; 32] };

		let encoded = encode_typed(
			test_fixtures::PEOPLE_METADATA,
			"Resources",
			"set_notification_statement_account_for_sequence",
			&args,
		);

		assert_eq!(encoded[2..], [7u32.to_le_bytes().to_vec(), vec![3], vec![0x22; 32]].concat());
	}

	#[test]
	fn the_pgas_call_has_the_reference_bytes() {
		let args = ClaimPgasCallArgs { slot_index: 4, target: [0x44; 32] };
		let expected =
			"6300040000004444444444444444444444444444444444444444444444444444444444444444";

		let encoded = encode_typed(test_fixtures::ASSET_HUB_METADATA, "Pgas", "claim_pgas", &args);

		assert_eq!(hex::encode(encoded), expected);
	}
}
