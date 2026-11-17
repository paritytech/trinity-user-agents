use super::*;

const MNEMONIC: &str =
	"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const DEVICE_ID: &str = "0101010101010101010101010101010101010101010101010101010101010101";

#[test]
fn profile_commands_distinguish_reads_from_uploads_and_preserve_quoted_paths() {
	assert_eq!(
		[
			"/profile",
			"/profile alice.01",
			r#"/profile upload "pictures/my avatar.png""#,
			"/profile upload"
		]
		.map(parse_command),
		[
			Ok(ShellCommand::Profile(ProfileCommand::Current)),
			Ok(ShellCommand::Profile(ProfileCommand::Get("alice.01".to_string()))),
			Ok(ShellCommand::Profile(ProfileCommand::Upload(PathBuf::from(
				"pictures/my avatar.png"
			)))),
			Err("usage: /profile [<username> | upload <image>]".into()),
		]
	);
}

#[test]
fn profile_completion_can_continue_inside_a_quoted_directory() {
	let directory = tempfile::tempdir().expect("create image directory");
	let pictures = directory.path().join("my pictures");
	fs::create_dir(&pictures).expect("create pictures directory");
	let avatar = pictures.join("avatar.png");
	fs::write(&avatar, []).expect("create image path");
	let input = format!("/profile upload {}/my", directory.path().display());
	let directories = completions_for_scope(&input, &[], CommandScope::SigningHost);
	let images = completions_for_scope(&directories[0].value, &[], CommandScope::SigningHost);

	assert_eq!(
		images
			.iter()
			.map(|completion| parse_command(&completion.value))
			.collect::<Vec<_>>(),
		vec![Ok(ShellCommand::Profile(ProfileCommand::Upload(avatar)))]
	);
}

#[test]
fn script_actions_preserve_paths_with_spaces_and_option_like_filenames() {
	let inputs = [
		"/script",
		"/script --edit",
		"/script --run",
		"/script --new",
		"/script --new projects/my example",
		"/script projects/my example/script.ts",
		"/script -- --run",
		"/script --other",
		"/script --new-example.ts",
	];

	assert_eq!(
		inputs.map(parse_command),
		[
			ScriptCommand::Edit,
			ScriptCommand::EditOnly,
			ScriptCommand::Run(None),
			ScriptCommand::New(None),
			ScriptCommand::New(Some(PathBuf::from("projects/my example"))),
			ScriptCommand::Run(Some(PathBuf::from("projects/my example/script.ts"))),
			ScriptCommand::Run(Some(PathBuf::from("--run"))),
			ScriptCommand::Run(Some(PathBuf::from("--other"))),
			ScriptCommand::Run(Some(PathBuf::from("--new-example.ts"))),
		]
		.map(|command| Ok(ShellCommand::Script(command)))
	);
}

#[test]
fn parses_all_operational_commands() {
	assert_eq!(parse_command("/pair"), Ok(ShellCommand::Pair(PairCommand::Scan)));
	assert_eq!(
		parse_command("/pair polkadotapp://pair?handshake=01"),
		Ok(ShellCommand::Pair(PairCommand::Deeplink(
			"polkadotapp://pair?handshake=01".to_string()
		)))
	);
	assert_eq!(
		parse_command(r#"/pair "/tmp/pairing QR.png""#),
		Ok(ShellCommand::Pair(PairCommand::Image(PathBuf::from("/tmp/pairing QR.png"))))
	);
	assert_eq!(
		parse_command("/script scripts/my smoke.ts"),
		Ok(ShellCommand::Script(ScriptCommand::Run(Some(PathBuf::from("scripts/my smoke.ts")))))
	);
	assert_eq!(parse_command("/script"), Ok(ShellCommand::Script(ScriptCommand::Edit)));
	assert_eq!(parse_command("/login"), Ok(ShellCommand::Login));
	assert_eq!(parse_command("/logout"), Ok(ShellCommand::Logout));
	assert_eq!(parse_command("/log trace"), Ok(ShellCommand::Log(LogLevel::Trace)));
	assert_eq!(parse_command("/product"), Ok(ShellCommand::Product(ProductCommand::Current)));
	assert_eq!(
		parse_command("/product Dotli.DOT"),
		Ok(ShellCommand::Product(ProductCommand::Switch(ProductId::for_test("dotli.dot"))))
	);
	assert_eq!(parse_command("/copy"), Ok(ShellCommand::Copy));
	assert_eq!(parse_command("/renew"), Ok(ShellCommand::Renew));
	assert_eq!(parse_command("/devices"), Ok(ShellCommand::Devices(DeviceCommand::List)));
	assert_eq!(
		parse_command(&format!("/devices --remove 0x{DEVICE_ID}")),
		Ok(ShellCommand::Devices(DeviceCommand::Remove {
			statement_account_id: [1; 32],
			force: false,
		}))
	);
	assert_eq!(
		parse_command(&format!("/devices --remove 0X{DEVICE_ID}")),
		Ok(ShellCommand::Devices(DeviceCommand::Remove {
			statement_account_id: [1; 32],
			force: false,
		}))
	);
	assert_eq!(
		parse_command(&format!("/devices --remove 0x{DEVICE_ID} --force")),
		Ok(ShellCommand::Devices(DeviceCommand::Remove {
			statement_account_id: [1; 32],
			force: true,
		}))
	);
	assert_eq!(parse_command("/session"), Ok(ShellCommand::Session(SessionCommand::Current)));
	assert_eq!(
		parse_command("/session alice"),
		Ok(ShellCommand::Session(SessionCommand::Switch("alice".to_string())))
	);
	assert_eq!(
		parse_command("/session --clear alice"),
		Ok(ShellCommand::Session(SessionCommand::Clear(sessions::SessionClearTarget::Named(
			"alice".to_string()
		))))
	);
	assert_eq!(
		parse_command("/session --clear-all"),
		Ok(ShellCommand::Session(SessionCommand::Clear(sessions::SessionClearTarget::All)))
	);
	let imported = parse_command(&format!("/session --mnemonic \"{MNEMONIC}\""))
		.expect("mnemonic command parses");
	let ShellCommand::Session(SessionCommand::ImportMnemonic(mnemonic)) = imported else {
		panic!("unexpected mnemonic command")
	};
	assert_eq!(mnemonic.expose_secret(), MNEMONIC);
}

#[test]
fn parses_runtime_approval_commands() {
	assert_eq!(parse_command("/approval"), Ok(ShellCommand::Approval(ApprovalCommand::Current)));
	assert_eq!(
		parse_command("/approval manual"),
		Ok(ShellCommand::Approval(ApprovalCommand::Manual))
	);
	assert_eq!(
		parse_command("/approval automatic"),
		Ok(ShellCommand::Approval(ApprovalCommand::Automatic))
	);
	assert_eq!(
		parse_command("/approval sometimes").unwrap_err().to_string(),
		"usage: /approval [manual|automatic]"
	);
}

#[test]
fn rejects_bare_and_malformed_commands() {
	assert!(parse_command("whoami").unwrap_err().to_string().contains("start with `/`"));
	assert!(parse_command("/whoami").is_err());
	assert!(parse_command("/copy now").is_err());
	assert!(parse_command("/login now").is_err());
	assert!(parse_command("/logout now").is_err());
	assert!(parse_command("/pair https://example.com").is_err());
	assert!(parse_command("/deeplink polkadotapp://pair?handshake=01").is_err());
	assert!(parse_command("/renew now").is_err());
	assert!(parse_command("/devices --remove").is_err());
	assert!(parse_command("/devices --remove not-an-account").is_err());
	assert!(parse_command(&format!("/devices --remove {DEVICE_ID} extra")).is_err());
	assert!(parse_command(&format!("/devices --remove --force {DEVICE_ID}")).is_err());
	assert!(parse_command(&format!("/devices --remove {DEVICE_ID} --force --force")).is_err());
	assert!(parse_command("/devices --force").is_err());
	assert!(parse_command("/devices --unknown").is_err());
	assert_eq!(
		parse_command("/log noisy"),
		Err(CommandParseError::LogLevel(InvalidLogLevel { value: "noisy".to_string() }))
	);
	assert_eq!(
		parse_command("/product example.com"),
		Err(CommandParseError::Product(ProductIdError::Invalid {
			input: "example.com".to_string()
		}))
	);
	assert_eq!(
		parse_command("/session ../escape"),
		Err(CommandParseError::Session(sessions::SessionNameError::FirstCharacter))
	);
	assert_eq!(
		parse_command("/session --clear").unwrap_err().to_string(),
		"usage: /session --clear <name>"
	);
	assert!(parse_command("/session --clear alice bob").is_err());
	assert!(parse_command("/session --clear-all now").is_err());
	assert!(parse_command("/session --unknown").is_err());
	assert!(parse_command("/session --mnemonic").is_err());
	assert!(parse_command("/session --mnemonic \"not closed").is_err());
	assert!(
		parse_command("/session default")
			.unwrap_err()
			.to_string()
			.contains("reserved for bootstrap state")
	);
}

#[test]
fn completion_selection_and_history_have_distinct_arrow_behavior() {
	let mut editor = CommandEditor::default();
	editor.set_text("/");
	let first = editor.completions()[0].value.clone();
	editor.down();
	assert_ne!(editor.completions()[editor.completion_index()].value, first);

	editor.dismiss_completions();
	editor.set_text("/help");
	editor.submit();
	editor.set_text("draft");
	editor.dismiss_completions();
	editor.up();
	assert_eq!(editor.text(), "/help");
	editor.down();
	assert_eq!(editor.text(), "draft");
}

#[test]
fn mnemonic_commands_are_redacted_and_never_enter_history() {
	let command = format!("/session --mnemonic \"{MNEMONIC}\"");
	let parsed = parse_command(&command).expect("mnemonic command parses");
	let rendered = format!("{parsed:?}");
	assert!(!rendered.contains("abandon"), "mnemonic leaked: {rendered}");
	assert!(rendered.contains("<redacted>"));

	let masked = mask_mnemonic(&command).expect("mnemonic is masked");
	assert!(masked.starts_with("/session --mnemonic "));
	assert!(!masked.contains("abandon"));
	assert_eq!(masked.chars().count(), command.chars().count());
	let irregular = format!("  /session\t --mnemonic\t{MNEMONIC}");
	assert!(contains_mnemonic(&irregular));
	assert!(
		!mask_mnemonic(&irregular)
			.expect("irregular whitespace is still masked")
			.contains("abandon")
	);

	let mut editor = CommandEditor::default();
	editor.set_text(command.clone());
	let rendered = format!("{editor:?}");
	assert!(!rendered.contains("abandon"), "editor leaked: {rendered}");
	assert_eq!(editor.submit(), command);
	assert!(editor.history.is_empty());
}

#[test]
fn editor_handles_unicode_at_character_boundaries() {
	let mut editor = CommandEditor::default();
	editor.set_text("/script café.ts");
	editor.left();
	editor.left();
	editor.left();
	editor.backspace();
	assert_eq!(editor.text(), "/script caf.ts");
}

#[test]
fn approval_parser_is_trimmed_and_case_insensitive() {
	assert_eq!(parse_approval(" YES "), Some(true));
	assert_eq!(parse_approval("n"), Some(false));
	assert_eq!(parse_approval("sure"), None);
}

#[test]
fn pair_command_advertises_image_paste_file_and_deeplink_inputs() {
	assert_eq!(
		completions_for_scope("/pair", &[], CommandScope::SigningHost),
		vec![Completion {
			value: "/pair".to_string(),
			description: "paste a pairing QR image or provide a file or URL",
		}]
	);
	assert!(HELP_TEXT.starts_with(
		"/pair                   paste or drop a pairing QR image\n\
         /pair <image-path>      read a pairing QR image file\n\
         /pair <url>             answer a Polkadot Mobile pairing URL"
	));
}

#[test]
fn script_completion_lists_matching_filesystem_paths() {
	let command = completions_for_scope("/script", &[], CommandScope::SigningHost);
	assert_eq!(command.len(), 1);
	assert_eq!(command[0].value, "/script");

	let prefix = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("signing_s");
	let matches = completions_for_scope(
		&format!("/script {}", prefix.display()),
		&[],
		CommandScope::SigningHost,
	);

	assert!(matches.iter().any(|completion| completion.value.ends_with("signing_shell.rs")));
}

#[test]
fn session_completion_lists_existing_names() {
	let matches = completions_for_scope(
		"/session a",
		&["alice".to_string(), "bob".to_string()],
		CommandScope::SigningHost,
	);

	assert_eq!(matches.len(), 1);
	assert_eq!(matches[0].value, "/session alice");
}

#[test]
fn device_completion_offers_list_and_remove_operations() {
	assert_eq!(
		completions_for_scope("/devices --", &[], CommandScope::SigningHost),
		vec![
			Completion { value: "/devices --list".to_string(), description: "list paired devices" },
			Completion {
				value: "/devices --remove".to_string(),
				description: "remove one paired device",
			},
		]
	);
	assert_eq!(
		completions_for_scope(
			&format!("/devices --remove {DEVICE_ID} --f"),
			&[],
			CommandScope::SigningHost
		),
		vec![Completion {
			value: format!("/devices --remove {DEVICE_ID} --force"),
			description: "remove locally if notification fails",
		}]
	);
	assert!(completions_for_scope("/devices", &[], CommandScope::PairingHost).is_empty());
}

#[test]
fn approval_completion_is_signing_host_only() {
	assert_eq!(
		completions_for_scope("/approval ", &[], CommandScope::SigningHost),
		vec![
			Completion {
				value: "/approval manual".to_string(),
				description: "prompt for every confirmation",
			},
			Completion {
				value: "/approval automatic".to_string(),
				description: "approve confirmations automatically",
			},
		]
	);
	assert!(completions_for_scope("/appro", &[], CommandScope::PairingHost).is_empty());
}

#[test]
fn session_completion_offers_clear_operations_and_existing_names() {
	let operations = completions_for_scope(
		"/session --c",
		&["alice".to_string(), "bob".to_string()],
		CommandScope::SigningHost,
	);
	assert!(operations.iter().any(|completion| completion.value == "/session --clear"));
	assert!(operations.iter().any(|completion| completion.value == "/session --clear-all"));
	let import =
		completions_for_scope("/session --m", &["alice".to_string()], CommandScope::SigningHost);
	assert_eq!(import.len(), 1);
	assert_eq!(import[0].value, "/session --mnemonic");

	let names = completions_for_scope(
		"/session --clear b",
		&["alice".to_string(), "bob".to_string()],
		CommandScope::SigningHost,
	);
	assert_eq!(names.len(), 1);
	assert_eq!(names[0].value, "/session --clear bob");
}

#[test]
fn pairing_host_completions_only_offer_shared_commands() {
	let matches = completions_for_scope("/", &[], CommandScope::PairingHost);
	let commands = matches.into_iter().map(|completion| completion.value).collect::<Vec<_>>();

	assert!(commands.contains(&"/script".to_string()));
	assert!(commands.contains(&"/login".to_string()));
	assert!(commands.contains(&"/logout".to_string()));
	assert!(commands.contains(&"/product".to_string()));
	assert!(commands.contains(&"/copy".to_string()));
	assert!(!commands.iter().any(|command| command.starts_with("/pair")));
	assert!(!commands.iter().any(|command| command.starts_with("/session")));
}

#[test]
fn log_completion_offers_levels_after_command_for_both_hosts() {
	for scope in [CommandScope::SigningHost, CommandScope::PairingHost] {
		let root_matches = completions_for_scope("/log", &[], scope);
		assert_eq!(root_matches.iter().filter(|completion| completion.value == "/log").count(), 1);
		assert!(!root_matches.iter().any(|completion| completion.value.starts_with("/log ")));

		let argument_matches = completions_for_scope("/log ", &[], scope);
		assert_eq!(
			argument_matches
				.into_iter()
				.map(|completion| completion.value)
				.collect::<Vec<_>>(),
			["/log error", "/log warn", "/log info", "/log debug", "/log trace",]
		);
	}
}
