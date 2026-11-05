use super::*;

/// Only the text `preimage_cid` writes names a preimage. Any other spelling of a CID, or a CID
/// of another kind, would have the core read or cache a blob under a name Bulletin does not use.
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
