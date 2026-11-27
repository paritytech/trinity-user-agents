use super::*;

#[test]
fn the_worker_node_is_the_namehash_of_the_worker_subname() {
	// `namehash("worker.fundtest-provider.paseo")`, worked out with an
	// independent keccak-256 namehash.
	let expected = "cff6672bf6122da501f10d5c4c701998861c18f382d05d3dd24cc7752bb514a2";
	assert_eq!(hex::encode(worker_node("paseo", "fundtest-provider")), expected);
	assert_eq!(hex::encode(worker_node(".paseo", "fundtest-provider")), expected);
}

#[test]
fn an_ipfs_contenthash_names_its_cid() {
	// What `worker.fundtest-provider.paseo` holds on Paseo Next v2.
	let contenthash =
		hex::decode("e30101701220d6cba87a2adedb415a5834355121850794b12409fab588dbcb3f01c055be2445")
			.unwrap();
	assert_eq!(
		contenthash_cid(&contenthash).unwrap().to_base32(),
		"bafybeigwzouhukw63navuwbugvisdbihssysicp2wwenxsz7ahaflpreiu"
	);
}

#[test]
fn a_contenthash_outside_ipfs_or_unset_names_no_worker() {
	assert!(contenthash_cid(&[]).is_err());
	// The `swarm` namespace.
	let swarm = [&[0xe4, 0x01, 0x01, 0xfa, 0x01, 0x1b, 0x20][..], &[0; 32]].concat();
	assert!(contenthash_cid(&swarm).is_err());
}

#[test]
fn the_manifest_entrypoint_stays_inside_the_archive() {
	assert_eq!(manifest_entrypoint(r#"{"entrypoint":"index.js"}"#).unwrap(), "index.js");
	assert_eq!(manifest_entrypoint(r#"{"entrypoint":"./dist/w.js"}"#).unwrap(), "dist/w.js");
	for refused in [
		"",
		"{}",
		r#"{"entrypoint":"/etc/passwd"}"#,
		r#"{"entrypoint":"../index.js"}"#,
		r#"{"entrypoint":"a/../../index.js"}"#,
		r#"{"entrypoint":""}"#,
	] {
		assert!(manifest_entrypoint(refused).is_err(), "{refused:?}");
	}
}

#[test]
fn a_stored_archive_holds_its_entry_script_and_is_kept_once() {
	let cache = tempfile::tempdir().unwrap();
	let directory = cache.path().join("archive");
	store(&directory, include_bytes!("fixtures/uploaded.car")).unwrap();
	let script = entry_script(&directory, "index.js").unwrap();
	assert_eq!(
		std::fs::read_to_string(script).unwrap(),
		"import { util } from \"./lib/util.js\";\nutil();\n"
	);
	assert!(directory.join("lib/util.js").is_file());
	assert!(entry_script(&directory, "missing.js").is_err());
	// A second store of the same archive finds it in place.
	store(&directory, include_bytes!("fixtures/uploaded.car")).unwrap();
	let entries: Vec<_> = std::fs::read_dir(cache.path()).unwrap().collect();
	assert_eq!(entries.len(), 1, "no staging directory is left behind");
}
