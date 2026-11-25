use super::*;

#[test]
fn the_resume_marker_follows_whether_funding_is_in_progress() {
	let directory = tempfile::tempdir().expect("temp dir");
	write_resume_marker(directory.path(), true);
	let pending = resume_marker(directory.path()).exists();
	write_resume_marker(directory.path(), false);
	write_resume_marker(directory.path(), false);
	assert_eq!((pending, resume_marker(directory.path()).exists()), (true, false));
}
