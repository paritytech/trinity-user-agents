use super::*;

#[tokio::test]
async fn own_profile_commands_require_the_active_signer_not_a_cached_username() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut session = unfinished_session(temporary.path(), "pending").await?;
	session.cached_user_id = Some("cached.01".to_string());

	let error = run_profile_command(&session, ProfileCommand::Upload("avatar.png".into()))
		.await
		.unwrap_err();

	assert_eq!(
		error.to_string(),
		"no active identity username; select an existing identity with /session <name>"
	);
	Ok(())
}
