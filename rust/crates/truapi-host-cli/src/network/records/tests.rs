use super::*;

/// A stack never fetched has no record, which is not an error: the core then
/// starts from the bundled copy or fetches the first one.
#[tokio::test]
async fn a_stack_with_no_record_reads_none() {
	let state = tempfile::tempdir().expect("temporary directory");
	let records = StackRecordFile::new(state.path().join("stack-unknown"));

	assert_eq!(records.read_stack_record("unknown").await, Ok(None));
}

/// A record that cannot be read is reported with its path, so the core logs
/// where to look before it starts without it.
#[tokio::test]
async fn a_record_that_cannot_be_read_names_its_path() {
	let state = tempfile::tempdir().expect("temporary directory");
	std::fs::create_dir(state.path().join(RECORD_FILE)).expect("a directory in the record's place");
	let records = StackRecordFile::new(state.path().to_path_buf());

	let error = records
		.read_stack_record("blocked")
		.await
		.expect_err("a directory is no record");

	assert!(error.reason.starts_with(&state.path().join(RECORD_FILE).display().to_string()));
}
