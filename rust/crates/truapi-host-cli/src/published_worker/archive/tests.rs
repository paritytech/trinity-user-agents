//! The fixtures are written by `../fixtures/make_fixtures.py`, an encoder
//! that shares no code with the decoder under test.

use super::*;

const DIRECTORY: &[u8] = include_bytes!("../fixtures/directory.car");
const UPLOADED: &[u8] = include_bytes!("../fixtures/uploaded.car");
const ESCAPE: &[u8] = include_bytes!("../fixtures/escape.car");

fn expected() -> BTreeMap<String, Vec<u8>> {
	BTreeMap::from([
		("index.js".to_string(), b"import { util } from \"./lib/util.js\";\nutil();\n".to_vec()),
		("lib/util.js".to_string(), b"export const util = () => 1;\n".to_vec()),
	])
}

#[test]
fn a_directory_archive_unpacks_every_file_with_its_chunks_in_order() {
	assert_eq!(unpack(DIRECTORY).unwrap(), expected());
}

#[test]
fn an_uploaded_car_file_unpacks_the_directory_it_holds() {
	assert!(looks_like_car(UPLOADED));
	assert_eq!(unpack(UPLOADED).unwrap(), expected());
}

#[test]
fn a_block_that_does_not_hash_to_its_cid_is_refused() {
	let mut tampered = DIRECTORY.to_vec();
	let at = tampered.windows(6).position(|window| window == b"util()").unwrap();
	tampered[at] = b'X';
	let error = unpack(&tampered).unwrap_err();
	assert!(format!("{error:#}").contains("does not hash to its CID"), "{error:#}");
}

#[test]
fn an_entry_that_leaves_the_archive_is_refused() {
	let error = unpack(ESCAPE).unwrap_err();
	assert!(format!("{error:#}").contains("not a plain name"), "{error:#}");
}

#[test]
fn bytes_that_are_not_a_car_are_refused() {
	assert!(!looks_like_car(b"export default 1;\n"));
	assert!(unpack(b"export default 1;\n").is_err());
}

#[test]
fn a_cid_round_trips_through_its_bytes_and_spells_as_a_gateway_names_it() {
	let bytes =
		hex::decode("01701220d6cba87a2adedb415a5834355121850794b12409fab588dbcb3f01c055be2445")
			.unwrap();
	let cid = Cid::parse(&bytes).unwrap();
	assert_eq!(cid.to_bytes(), bytes);
	assert_eq!(cid.to_base32(), "bafybeigwzouhukw63navuwbugvisdbihssysicp2wwenxsz7ahaflpreiu");
}

#[test]
fn a_cidv0_names_the_same_node_as_its_cidv1() {
	let digest = [7u8; 32];
	let v0 = [&[0x12, 0x20][..], &digest].concat();
	let v1 = [&[0x01, 0x70, 0x12, 0x20][..], &digest].concat();
	assert_eq!(Cid::parse(&v0).unwrap(), Cid::parse(&v1).unwrap());
}
