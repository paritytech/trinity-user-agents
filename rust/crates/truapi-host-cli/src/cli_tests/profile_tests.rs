use super::*;

#[tokio::test]
async fn own_profile_commands_require_the_active_signer_not_a_cached_username() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut session = unfinished_session(temporary.path(), "pending").await?;
	session.cached_user_id = Some("cached.01".to_string());
	for command in [ProfileCommand::Current, ProfileCommand::Upload("avatar.png".into())] {
		let error = run_profile_command(&session, command).await.unwrap_err();
		assert_eq!(
			error.to_string(),
			"no active identity username; select an existing identity with /session <name>"
		);
	}
	assert_eq!(
		(
			session.signer.is_some(),
			accounts::has_account_store(&session.profile.as_ref().unwrap().account_base_path),
		),
		(false, false)
	);
	Ok(())
}

#[tokio::test]
async fn profile_identity_follows_the_active_signer() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut session = unfinished_session(temporary.path(), "pending").await?;
	session.cached_user_id = Some("cached.01".to_string());
	session.signer = Some(ResolvedSigner {
		entropy: vec![7; 16],
		account_name: None,
		lite_username: Some("active.01".to_string()),
		auto_managed: false,
	});

	let (signer, username) = profile_identity(&session)?;
	assert_eq!((username, signer.entropy.as_slice()), ("active.01", [7; 16].as_slice()));
	Ok(())
}
