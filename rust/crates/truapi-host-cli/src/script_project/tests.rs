use super::*;

#[test]
fn new_projects_carry_the_matching_declarations_without_reading_a_checkout() -> Result<()> {
	let root = tempfile::tempdir()?;
	let script = create_with_types(root.path(), None, b"installed runner types")?;
	assert_eq!(fs::read(script.with_extension("types.d.ts"))?, b"installed runner types");
	Ok(())
}

/// The project root sits beside the host state under the base path, so a
/// first `/script` must not create the base path readable by other users
/// when the state directory would have made it owner-only.
#[cfg(unix)]
#[test]
fn the_project_root_and_its_new_parents_are_readable_by_the_owner_only() -> Result<()> {
	use std::os::unix::fs::PermissionsExt;

	let temporary = tempfile::tempdir()?;
	let base = temporary.path().join("truapi-host");
	create_with_types(&base.join("scripts"), None, b"types")?;

	let mode =
		|path: &Path| -> Result<u32> { Ok(fs::metadata(path)?.permissions().mode() & 0o777) };
	assert_eq!((mode(&base)?, mode(&base.join("scripts"))?), (0o700, 0o700));
	Ok(())
}

#[test]
fn ordinary_projects_keep_the_callers_working_directory() -> Result<()> {
	let root = tempfile::tempdir()?;
	fs::write(root.path().join("package.json"), r#"{"name":"ordinary-project"}"#)?;
	assert!(directory(&root.path().join("script.ts"))?.is_none());
	Ok(())
}

#[test]
fn creating_a_project_never_overwrites_an_existing_directory() -> Result<()> {
	let root = tempfile::tempdir()?;
	let destination = root.path().join("my project");
	fs::create_dir(&destination)?;
	fs::write(destination.join("script.ts"), "user work")?;

	assert!(create_with_types(root.path(), Some(&destination), b"types").is_err());
	assert_eq!(fs::read_to_string(destination.join("script.ts"))?, "user work");
	Ok(())
}

#[test]
fn project_manifest_can_resolve_a_renamed_entrypoint() -> Result<()> {
	let root = tempfile::tempdir()?;
	let script = create_with_types(root.path(), None, b"types")?;
	let directory = script.parent().unwrap();
	let manifest_path = directory.join("package.json");
	let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
	manifest["truapiHost"]["script"] = "renamed.ts".into();
	fs::write(manifest_path, serde_json::to_vec(&manifest)?)?;
	let renamed = directory.join("renamed.ts");
	fs::rename(&script, &renamed)?;

	assert_eq!(remembered_script(&script)?, Some(renamed));
	Ok(())
}

#[test]
fn dependency_setup_retries_partial_or_changed_installations() -> Result<()> {
	let root = tempfile::tempdir()?;
	fs::write(root.path().join("package.json"), r#"{"dependencies":{"example":"1.0.0"}}"#)?;
	fs::write(root.path().join("bun.lock"), "locked")?;
	fs::create_dir_all(root.path().join("node_modules/example"))?;
	fs::write(root.path().join("node_modules/example/package.json"), "{}")?;
	assert!(!installed(root.path())?);
	fs::write(root.path().join(INSTALL_STAMP), fingerprint(root.path())?.unwrap())?;
	assert!(installed(root.path())?);
	fs::remove_file(root.path().join("node_modules/example/package.json"))?;
	assert!(!installed(root.path())?);
	fs::write(root.path().join("node_modules/example/package.json"), "{}")?;
	fs::write(root.path().join("bun.lock"), "updated")?;
	assert!(!installed(root.path())?);
	Ok(())
}
