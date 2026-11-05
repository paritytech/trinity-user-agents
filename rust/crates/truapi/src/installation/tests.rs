use super::*;

/// Keys derived under a truncated or padded id would land in a subtree
/// recovery never scans, so only exactly 32 bytes make an id.
#[test]
fn an_installation_id_takes_exactly_32_bytes() {
	let read =
		|length: usize| InstallationId::try_from(vec![7; length]).map_err(|error| error.reason);
	assert_eq!(
		[read(31), read(32), read(33)],
		[
			Err("an installation id has 32 bytes, got 31".to_owned()),
			Ok(InstallationId([7; 32])),
			Err("an installation id has 32 bytes, got 33".to_owned()),
		]
	);
}
