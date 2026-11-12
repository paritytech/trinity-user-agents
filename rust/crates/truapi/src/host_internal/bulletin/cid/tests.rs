use parity_scale_codec::{DecodeAll, Encode};

use super::*;

/// A blob stored on Paseo Bulletin: `bitswap_v1_get` with this CID returns the bytes whose
/// blake2b-256 is the key.
#[test]
fn a_key_displays_as_its_cidv1_raw_blake2b_256_in_base32() {
	let key: [u8; 32] =
		hex::decode("0bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c")
			.unwrap()
			.try_into()
			.unwrap();
	assert_eq!(
		BulletinCid::from(key).to_string(),
		"bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy"
	);
}

/// Only the text a `BulletinCid` displays as names a preimage. Any other spelling of a CID, or a
/// CID of another kind, would have the core read or cache a blob under a name Bulletin does not
/// use.
#[test]
fn only_a_canonical_blake2b_cid_parses() {
	let cid = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy";
	let sha2_cid = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";
	let dag_pb_cid = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";

	assert_eq!(
		[cid, &cid.to_uppercase(), &cid[..cid.len() - 1], sha2_cid, dag_pb_cid]
			.map(|text| text.parse::<BulletinCid>().map(|parsed| hex::encode(parsed.0))),
		[
			Ok("0bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c".to_string()),
			Err(BulletinCidError::Encoding),
			Err(BulletinCidError::Length),
			Err(BulletinCidError::Hash),
			Err(BulletinCidError::Codec),
		]
	);
}

#[test]
fn scale_encodes_only_the_digest() {
	let text = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy";
	let cid = text.parse::<BulletinCid>().unwrap();
	let digest =
		hex::decode("0bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c").unwrap();

	assert_eq!(
		(cid.encode(), BulletinCid::decode_all(&mut digest.as_slice()).unwrap()),
		(digest, cid),
	);
}

#[test]
fn scale_requires_a_complete_digest() {
	assert_eq!(
		[0, 31, 33].map(|length| BulletinCid::decode_all(&mut vec![0; length].as_slice()).is_err()),
		[true; 3],
	);
}
