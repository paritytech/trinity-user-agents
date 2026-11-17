use super::*;

fn payload() -> HostSignPayloadData {
	HostSignPayloadData {
		block_hash: vec![0xB1, 0xB2],
		block_number: vec![0xFF],
		era: vec![0xE1],
		genesis_hash: vec![0x61, 0x62],
		method: vec![0x4D],
		nonce: vec![0x4E],
		spec_version: vec![0x51],
		tip: vec![0x54],
		transaction_version: vec![0x56],
		signed_extensions: vec![
			"CheckSpecVersion".to_string(),
			"CheckTxVersion".to_string(),
			"CheckGenesis".to_string(),
			"CheckMortality".to_string(),
			"CheckNonce".to_string(),
			"ChargeTransactionPayment".to_string(),
		],
		version: 4,
		asset_id: None,
		metadata_hash: None,
		mode: None,
		with_signed_transaction: parity_scale_codec::OptionBool(None),
	}
}

#[test]
fn payload_preimage_uses_extrinsic_payload_v4_field_order() {
	// method, era, nonce, tip, spec_version, transaction_version,
	// genesis_hash, block_hash. block_number is not part of the preimage.
	assert_eq!(
		extrinsic_payload_preimage(&payload()).unwrap(),
		vec![0x4D, 0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]
	);
}

#[test]
fn payload_preimage_places_asset_id_after_tip_and_metadata_hash_last() {
	let mut payload = payload();
	payload.signed_extensions = vec![
		"CheckSpecVersion".to_string(),
		"CheckTxVersion".to_string(),
		"CheckGenesis".to_string(),
		"CheckMortality".to_string(),
		"CheckNonce".to_string(),
		"ChargeAssetTxPayment".to_string(),
		"CheckMetadataHash".to_string(),
	];
	payload.asset_id = Some(vec![0x01, 0xAA]);
	payload.mode = Some(1);
	payload.metadata_hash = Some(vec![0xBB; 32]);

	let mut expected = vec![
		0x4D, 0xE1, 0x4E, 0x54, // method, era, nonce, tip
		0x01, 0xAA, // asset_id (TAssetConversion bytes)
		0x01, // mode
		0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2, // spec, tx, genesis, block
		0x01, // Some(metadata_hash)
	];
	expected.extend_from_slice(&[0xBB; 32]);
	assert_eq!(extrinsic_payload_preimage(&payload).unwrap(), expected);
}

#[test]
fn payload_preimage_defaults_listed_extensions_to_disabled() {
	// A chain that has the extensions while the payload leaves them unset
	// still signs their default encodings: assetId None, mode 0,
	// metadata_hash None.
	let mut payload = payload();
	payload.signed_extensions = vec![
		"CheckSpecVersion".to_string(),
		"CheckTxVersion".to_string(),
		"CheckGenesis".to_string(),
		"CheckMortality".to_string(),
		"CheckNonce".to_string(),
		"ChargeAssetTxPayment".to_string(),
		"CheckMetadataHash".to_string(),
	];
	payload.mode = Some(0);

	assert_eq!(
		extrinsic_payload_preimage(&payload).unwrap(),
		vec![
			0x4D, 0xE1, 0x4E, 0x54, // method, era, nonce, tip
			0x00, // asset_id None
			0x00, // mode 0
			0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2, // spec, tx, genesis, block
			0x00, // metadata_hash None
		]
	);
}

#[test]
fn payload_preimage_ignores_mode_without_check_metadata_hash() {
	// polkadot-js always emits `mode: 0` in the payload JSON, even for
	// chains without CheckMetadataHash; the extension list decides.
	let mut payload = payload();
	payload.mode = Some(0);

	assert_eq!(
		extrinsic_payload_preimage(&payload).unwrap(),
		vec![0x4D, 0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]
	);
}

#[test]
fn payload_preimage_follows_noncanonical_runtime_order() {
	let mut payload = payload();
	payload.signed_extensions = vec![
		"CheckNonce".to_string(),
		"CheckMortality".to_string(),
		"CheckGenesis".to_string(),
		"CheckSpecVersion".to_string(),
	];

	assert_eq!(
		extrinsic_payload_preimage(&payload).unwrap(),
		vec![
			0x4D, 0x4E, 0xE1, // method, nonce extra, era extra
			0xB1, 0xB2, // mortality implicit
			0x61, 0x62, // genesis implicit
			0x51, // spec-version implicit
		]
	);
}

#[test]
fn payload_preimage_rejects_unsupported_extension() {
	let mut payload = payload();
	payload.signed_extensions = vec!["CustomExtension".to_string()];

	assert_eq!(
		extrinsic_payload_preimage(&payload),
		Err(ExtrinsicPayloadError::UnsupportedSignedExtension {
			id: "CustomExtension".to_string()
		})
	);
}

#[test]
fn payload_preimage_rejects_out_of_range_mode() {
	let mut payload = payload();
	payload.signed_extensions = vec!["CheckMetadataHash".to_string()];
	payload.mode = Some(256);

	assert_eq!(
		extrinsic_payload_preimage(&payload),
		Err(ExtrinsicPayloadError::MetadataHashModeOutOfRange { mode: 256 })
	);
}

#[test]
fn payload_preimage_rejects_invalid_metadata_hash_length() {
	let mut payload = payload();
	payload.signed_extensions = vec!["CheckMetadataHash".to_string()];
	payload.metadata_hash = Some(vec![0xBB; 31]);

	assert_eq!(
		extrinsic_payload_preimage(&payload),
		Err(ExtrinsicPayloadError::InvalidMetadataHashLength { len: 31 })
	);
}

#[test]
fn long_preimages_are_blake2b_hashed() {
	let mut payload = payload();
	payload.method = vec![0x4D; 300];

	let preimage = extrinsic_payload_preimage(&payload).unwrap();

	assert_eq!(preimage.len(), 32);
	let mut raw = vec![0x4D; 300];
	raw.extend_from_slice(&[0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]);
	assert_eq!(
		preimage,
		blake2b_simd::Params::new().hash_length(32).hash(&raw).as_bytes().to_vec()
	);
}
