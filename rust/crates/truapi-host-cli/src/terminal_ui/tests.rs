use super::*;
use crossterm::event::MouseEvent;
use qrcode::Color as QrColor;

/// The quiet zone a QR code keeps around it, in modules.
const QR_QUIET_ZONE: usize = 4;
use ratatui::{backend::TestBackend, layout::Position};
use tracing_subscriber::layer::SubscriberExt;

/// Serializes the tests that install their own sender into the active UI
/// slot, so one test's install cannot displace a sender another test is
/// still reading from.
static ACTIVE_UI_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Claim the active UI slot for the duration of a test. A test that panics
/// while holding the slot poisons the lock; recover so the panic stays
/// local to that test instead of cascading into the next one.
fn lock_active_ui() -> std::sync::MutexGuard<'static, ()> {
	ACTIVE_UI_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn test_app() -> App {
	App::new(
		"testnet".to_string(),
		ChainBackend::Rpc,
		ProductId::for_test("playground.dot"),
		"default".to_string(),
		vec!["default".to_string()],
		"info".to_string(),
	)
}

#[test]
fn approval_preserves_the_decision_and_restores_command_draft() {
	for (kind, key, expected) in [
		(ApprovalKind::Action, KeyCode::Char('y'), PermissionDecision::AllowAlways),
		(ApprovalKind::Permission, KeyCode::Char('o'), PermissionDecision::AllowOnce),
		(ApprovalKind::Permission, KeyCode::Char('a'), PermissionDecision::AllowAlways),
		(ApprovalKind::Permission, KeyCode::Char('n'), PermissionDecision::Deny),
		(ApprovalKind::Permission, KeyCode::Esc, PermissionDecision::Deny),
		(ApprovalKind::Permission, KeyCode::Enter, PermissionDecision::Deny),
	] {
		let mut app = test_app();
		app.editor.set_text("/script draft.ts");
		let (response, answer) = oneshot::channel();
		app.handle_event(UiEvent::Approval {
			action: "remote permission".to_string(),
			detail: "access to api.example.com".to_string(),
			kind,
			response,
		});
		assert!(app.transcript_text().contains(kind.choices()));
		app.handle_approval_key(KeyEvent::new(key, KeyModifiers::NONE));
		assert_eq!(
			(answer.blocking_recv(), app.editor.text(), app.pending_approval.is_none()),
			(Ok(expected), "/script draft.ts".to_string(), true)
		);
	}
}

#[test]
fn ctrl_u_and_ctrl_d_scroll_half_a_viewport() {
	let mut app = test_app();
	app.transcript_height = 20;
	app.max_scroll_from_bottom = 100;
	app.handle_idle_event(Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)));
	assert_eq!(app.scroll_from_bottom, 10);
	app.handle_idle_event(Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)));
	assert_eq!(app.scroll_from_bottom, 0);
}

#[test]
fn page_keys_scroll_without_cancelling_a_running_command() {
	let mut app = test_app();
	app.transcript_height = 20;
	app.max_scroll_from_bottom = 100;
	app.busy = Some("/login".to_string());

	assert!(footer_text(&app, false, false, 120).contains("PgUp/PgDn scroll"));

	assert!(
		!app.handle_busy_event(Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE,)))
	);
	assert_eq!(app.scroll_from_bottom, 20);

	assert!(
		!app.handle_busy_event(Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE,)))
	);
	assert_eq!(app.scroll_from_bottom, 0);
}

#[test]
fn ctrl_v_is_reserved_for_clipboard_images() {
	assert!(is_clipboard_image_paste(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL,)));
	assert!(!is_clipboard_image_paste(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE,)));
}

#[test]
fn normal_paste_and_enter_request_pairing_image_input() {
	assert_eq!(
		(
			pairing_image_request(&Event::Paste("clipboard text".to_string())),
			pairing_image_request(&Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE,))),
		),
		(Some(PairingImageRequest::Clipboard), Some(PairingImageRequest::SubmitPath),)
	);
}

#[test]
fn dragged_image_paths_accept_terminal_text_formats() {
	let directory = tempfile::tempdir().expect("create temporary directory");
	let path = directory.path().join("pairing code.png");
	std::fs::write(&path, []).expect("create image path");
	let raw = path.display().to_string();
	let quoted = format!("'{raw}'");
	let escaped = raw.replace(' ', "\\ ");
	let file_url = reqwest::Url::from_file_path(&path)
		.expect("convert path to file URL")
		.to_string();

	assert_eq!(
		[raw, quoted, escaped, file_url].map(|text| { pairing_image_request(&Event::Paste(text)) }),
		std::array::from_fn(|_| Some(PairingImageRequest::Path(path.clone())))
	);
}

#[test]
fn invalid_text_paste_explains_clipboard_and_file_recovery() {
	let error = anyhow::anyhow!("clipboard does not contain an image");

	assert_eq!(
            (
                pairing_image_failure(&error, true),
                pairing_image_failure(&error, false),
            ),
            (
                "clipboard does not contain an image; pasted text is not a readable image file path. Copy an image or drag an image file here."
                    .to_string(),
                "clipboard does not contain an image".to_string(),
            )
        );
}

#[test]
fn clipboard_image_errors_explain_empty_and_unreadable_contents() {
	assert_eq!(
		(
			clipboard_image_error(arboard::Error::ContentNotAvailable).to_string(),
			clipboard_image_error(arboard::Error::ConversionFailure).to_string(),
		),
		(
			"clipboard does not contain an image".to_string(),
			"clipboard image could not be converted to RGBA pixels".to_string(),
		)
	);
}

#[test]
fn mouse_wheel_and_end_navigate_while_an_approval_is_pending() {
	let mut app = test_app();
	app.transcript_height = 20;
	app.max_scroll_from_bottom = 100;
	let (response, _answer) = oneshot::channel();
	app.handle_event(UiEvent::Approval {
		action: "sign request".to_string(),
		detail: "payload".to_string(),
		kind: ApprovalKind::Action,
		response,
	});
	assert!(footer_text(&app, true, false, 120).contains("PgUp/PgDn scroll"));

	app.handle_idle_event(Event::Mouse(MouseEvent {
		kind: MouseEventKind::ScrollUp,
		column: 0,
		row: 0,
		modifiers: KeyModifiers::NONE,
	}));
	assert_eq!(app.scroll_from_bottom, MOUSE_SCROLL_LINES);

	app.handle_idle_event(Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)));
	assert_eq!(app.scroll_from_bottom, 0);
}

#[test]
fn overflowing_transcript_advertises_scroll_controls_before_first_scroll() -> Result<()> {
	let mut app = test_app();
	for index in 0..20 {
		app.stream(StreamKind::Stdout, format!("retained line {index}"));
	}

	let (screen, _) = render_app(&mut app, 120, 10)?;

	assert!(screen.contains("PgUp/PgDn · wheel scroll"));
	Ok(())
}

#[test]
fn scroll_guidance_takes_space_from_a_long_status_line() {
	let mut app = test_app();
	app.product =
		Some(ProductId::for_test("product-name-that-would-otherwise-fill-the-status-line.dot"));
	app.max_scroll_from_bottom = 10;

	let status = line_text(composer_status_line(&app, false, false, 80));

	assert!(status.contains("PgUp/PgDn"));
	assert!(text_display_width(&status) <= 80);
}

#[test]
fn transcript_is_bounded() {
	let mut app = test_app();
	for index in 0..=TRANSCRIPT_LIMIT {
		app.push(FeedItem::Log(index.to_string()));
	}
	assert_eq!(app.entries.len(), TRANSCRIPT_LIMIT);
	assert!(matches!(app.entries.front(), Some(FeedItem::Log(text)) if text == "1"));
}

#[test]
fn transcript_copy_uses_natural_grouped_output_and_redacts_secrets() {
	let mut app = test_app();
	app.handle_system_event(SystemEvent::SigningHostReady);
	app.push_command("/pair polkadotapp://pair?handshake=secret".to_string());
	app.push_command("/pair \"/tmp/pairing QR.png\"".to_string());
	app.push_command("/session --mnemonic \"abandon abandon abandon about\"".to_string());
	app.stream(StreamKind::Stdout, "user id: alice.dot".to_string());

	let transcript = app.transcript_text();
	assert!(transcript.contains("✓ Signing host ready"));
	assert!(transcript.contains("─ /pair <pairing link>"));
	assert!(transcript.contains("─ /pair \"/tmp/pairing QR.png\""));
	assert!(transcript.contains("─ /session --mnemonic <redacted>"));
	assert!(transcript.contains("  user id: alice.dot"));
	assert!(!transcript.contains("handshake=secret"));
	assert!(!transcript.contains("abandon"));
	assert!(!transcript.contains("SCRIPT ·"));
}

#[test]
fn provisioning_is_announced_so_a_supervised_host_is_not_mistaken_for_a_dead_one() {
	let mut app = test_app();

	app.handle_system_event(SystemEvent::SigningHostProvisioning);

	assert_eq!(
		app.transcript_text(),
		"• Provisioning a signer\n  Registering a username on chain. This takes minutes on a first run."
	);
}

#[test]
fn missing_signing_session_explains_how_to_connect_a_user() {
	let mut app = test_app();

	app.handle_system_event(SystemEvent::SigningHostNeedsSession);

	assert_eq!(
		app.transcript_text(),
		"! No connected user\n  Use /session <name> to start a session."
	);
}

#[test]
fn keyed_activity_updates_in_place() {
	let mut app = test_app();
	for attempt in 1..=90 {
		app.activity(
			"pairing".to_string(),
			"Preparing pairing".to_string(),
			Some(format!("Attempt {attempt}/90")),
			ActivityState::Running,
		);
	}
	app.activity(
		"pairing".to_string(),
		"Paired with alice.dot".to_string(),
		None,
		ActivityState::Succeeded,
	);

	assert_eq!(app.entries.len(), 1);
	assert!(app.transcript_text().contains("✓ Paired with alice.dot"));
}

#[test]
fn repeated_activity_keeps_completed_history() {
	let mut app = test_app();
	app.start_activity("pairing".to_string(), "Preparing pairing".to_string(), None);
	app.activity(
		"pairing".to_string(),
		"Paired with alice.dot".to_string(),
		None,
		ActivityState::Succeeded,
	);
	app.start_activity("pairing".to_string(), "Preparing another pairing".to_string(), None);
	app.activity(
		"pairing".to_string(),
		"Paired with bob.dot".to_string(),
		None,
		ActivityState::Succeeded,
	);

	assert_eq!(app.entries.len(), 2);
	let transcript = app.transcript_text();
	assert!(transcript.contains("✓ Paired with alice.dot"));
	assert!(transcript.contains("✓ Paired with bob.dot"));
}

#[test]
fn failed_operation_finalizes_new_running_activities() {
	let mut app = test_app();
	let checkpoint = app.next_item_id;
	app.activity(
		"signer".to_string(),
		"Setting up signer".to_string(),
		Some("Waiting for identity".to_string()),
		ActivityState::Running,
	);
	app.finish_running_activities_since(
		checkpoint,
		ActivityState::Failed,
		"Stopped after an error",
	);

	assert!(app.transcript_text().contains("× Setting up signer"));
	assert!(app.transcript_text().contains("Stopped after an error"));
	assert!(!app.entries.iter().any(|entry| {
		matches!(entry, FeedItem::Activity { state: ActivityState::Running, .. })
	}));
}

#[test]
fn interactive_error_preserves_backend_cause_chain() {
	let (sender, receiver) = mpsc::unbounded_channel();
	let mut ui = ActiveTerminalUi {
		terminal: None,
		events: None,
		receiver,
		sender,
		app: test_app(),
		clipboard: None,
		copy_next_pairing_deeplink: false,
	};
	let error = anyhow::anyhow!("backend supplied explanation")
		.context("username registration failed (503 Service Unavailable)")
		.context("attest account auto-1");

	ui.error_with_causes(&error);

	assert_eq!(
		ui.app.transcript_text(),
		"× attest account auto-1\n  username registration failed (503 Service Unavailable)\n  backend supplied explanation"
	);
}

#[test]
fn script_streams_group_lines_and_preserve_blank_lines() {
	let mut app = test_app();
	app.stream(StreamKind::Stdout, "first".to_string());
	app.stream(StreamKind::Stdout, String::new());
	app.stream(StreamKind::Stdout, "third".to_string());
	app.stream(StreamKind::Stderr, "failed".to_string());

	assert_eq!(app.entries.len(), 2);
	assert!(matches!(
		app.entries.front(),
		Some(FeedItem::Stream { lines, .. }) if lines == &["first", "", "third"]
	));
	assert_eq!(app.transcript_text(), "  first\n  \n  third\n! failed");
}

#[test]
fn large_script_output_is_chunked_and_bounded() {
	let mut app = test_app();
	for index in 0..(TRANSCRIPT_LINE_LIMIT + 500) {
		app.stream(StreamKind::Stdout, format!("line {index}"));
	}

	assert!(app.retained_lines <= TRANSCRIPT_LINE_LIMIT);
	assert!(app.retained_bytes <= TRANSCRIPT_BYTE_LIMIT);
	assert!(app.entries.iter().all(|entry| {
		match entry {
			FeedItem::Stream { lines, .. } => lines.len() <= STREAM_CHUNK_LINE_LIMIT,
			_ => true,
		}
	}));
	assert!(app.transcript_text().contains("line 10499"));
	assert!(!app.transcript_text().contains("line 0\n"));
}

#[test]
fn autocomplete_shows_all_current_commands_and_scrolls_larger_menus() {
	let mut app = test_app();
	app.editor.set_text("/");
	let completions = app.editor.completions();
	let visible = completion_window(
		completions.len(),
		app.editor.completion_index(),
		usize::from(MAX_VISIBLE_COMPLETIONS),
	);
	assert!(visible.len() <= usize::from(MAX_VISIBLE_COMPLETIONS));
	assert!(completions.iter().any(|completion| completion.value == "/copy"));
	assert!(completions.iter().any(|completion| completion.value == "/renew"));

	assert_eq!(completion_window(12, 0, 8), 0..8);
	assert_eq!(completion_window(12, 11, 8), 4..12);
	assert_eq!(completion_window(12, 11, 0), 0..0);
}

#[test]
fn autocomplete_descriptions_share_one_column() -> Result<()> {
	let mut app = test_app();
	app.editor.set_text("/");

	let (screen, _) = render_app(&mut app, 80, 16)?;
	let pair = screen
		.lines()
		.find(|line| line.contains("paste a pairing"))
		.context("render pair completion")?;
	let script = screen
		.lines()
		.find(|line| line.contains("edit the last"))
		.context("render script completion")?;
	let renew = screen
		.lines()
		.find(|line| line.contains("renew statement-store"))
		.context("render renew completion")?;

	let column = |line: &str, description: &str| {
		line.find(description).map(|index| text_display_width(&line[..index]))
	};
	assert_eq!(column(pair, "paste"), column(script, "edit"));
	assert_eq!(column(script, "edit"), column(renew, "renew statement-store"));
	Ok(())
}

#[test]
fn session_event_updates_status_state_and_completions() {
	let mut app = test_app();
	app.handle_event(UiEvent::Session {
		name: "alice".to_string(),
		available: vec!["alice".to_string(), "bob".to_string()],
	});
	app.editor.set_text("/session b");

	assert_eq!(app.session, "alice");
	assert_eq!(app.editor.completions()[0].value, "/session bob");
}

#[test]
fn session_prefix_enter_submits_the_typed_name() {
	let mut app = test_app();
	app.editor
		.set_session_names(vec!["workbench.42".to_string(), "workbench.73".to_string()]);
	app.editor.set_text("/session workbench");
	let submitted =
		app.handle_idle_event(Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

	assert_eq!(submitted, Some(Some("/session workbench".to_string())));
}

#[test]
fn pairing_host_status_shows_the_selected_log_level() -> Result<()> {
	let mut app = App::new_pairing(
		"paseo-next-v2".to_string(),
		ChainBackend::Rpc,
		ProductId::for_test("playground.dot"),
		"info".to_string(),
	);
	app.editor.set_text("/");

	let (screen, _) = render_app(&mut app, 120, 14)?;
	let status = line_text(header_line(&app, 120));

	assert_eq!(
		status,
		" TrUAPI pairing host · 👤 disconnected · 🌐 paseo-next-v2 (rpc) · 📦 playground.dot · log info"
	);
	assert!(screen.contains("log info"));
	assert!(screen.contains("/script"));
	assert!(!screen.contains("/pair"));
	assert!(!screen.contains("/session"));
	Ok(())
}

#[test]
fn signing_host_status_shows_user_network_product_and_log_level() -> Result<()> {
	let mut app = test_app();
	app.connection = "alice.dot".to_string();

	let (screen, _) = render_app(&mut app, 120, 8)?;
	let status = line_text(header_line(&app, 120));

	assert_eq!(
		status,
		" TrUAPI signing host · 👤 alice.dot · 🌐 testnet (rpc) · 📦 playground.dot · log info"
	);
	assert!(!screen.contains("session default"));
	Ok(())
}

#[test]
fn changed_log_level_is_visible_in_the_status_bar() {
	let mut app = test_app();

	app.log_filter = "trace".to_string();

	assert!(line_text(header_line(&app, 120)).ends_with(" · log trace"));
}

#[test]
fn rust_log_filter_value_is_visible_in_the_status_bar() {
	let app = App::new(
		"testnet".to_string(),
		ChainBackend::Rpc,
		ProductId::for_test("playground.dot"),
		"default".to_string(),
		vec!["default".to_string()],
		"warn,truapi=debug".to_string(),
	);

	assert!(line_text(header_line(&app, 120)).ends_with(" · log warn,truapi=debug"));
}

#[test]
fn narrow_status_prioritizes_the_log_level() {
	let app = App::new(
		"testnet".to_string(),
		ChainBackend::Rpc,
		ProductId::for_test("playground.dot"),
		"default".to_string(),
		vec!["default".to_string()],
		"debug".to_string(),
	);

	assert_eq!(line_text(header_line(&app, 10)), " log debug");
}

/// Nothing else in the UI says which backend serves the chains, so a narrow
/// bar shortens the network name and keeps the backend whole.
#[test]
fn a_narrow_status_bar_keeps_the_chain_backend() {
	let mut app = test_app();
	app.network = "paseo-next-v2".to_string();
	app.chain_backend = ChainBackend::LightClient;
	app.connection = "alice.dot".to_string();

	assert_eq!(
		[80, 60].map(|width| line_text(header_line(&app, width))),
		[
			" 👤 alice.dot · 🌐 paseo-next-v2 (light-client) · 📦 playground.dot · log info",
			" 👤 alice… · 🌐 paseo… (light-client) · 📦 playg… · log info",
		]
	);
}

#[test]
fn long_product_name_is_ellipsized_in_the_status_bar() -> Result<()> {
	let mut app = test_app();
	app.product =
		Some(ProductId::for_test("product-name-that-is-far-too-long-for-the-status-bar.dot"));

	let status = line_text(header_line(&app, 80));

	assert!(status.contains('…'));
	assert!(!status.contains(app.product.as_ref().map(ProductId::as_str).unwrap_or_default()));
	assert!(status.ends_with(" · log info"));
	assert!(text_display_width(&status) <= 80);
	for width in 1..=120 {
		let status = line_text(header_line(&app, width));
		assert!(text_display_width(&status) <= usize::from(width));
	}
	Ok(())
}

#[test]
fn idle_command_guidance_is_an_input_placeholder() -> Result<()> {
	let mut app = test_app();

	let (idle, _) = render_app(&mut app, 80, 7)?;
	assert!(idle.contains("› Type / for commands"));
	assert!(!idle.contains("↑↓ history"));

	app.editor.set_text("/session");
	let (editing, _) = render_app(&mut app, 80, 7)?;
	assert!(!editing.contains("Type / for commands"));
	Ok(())
}

/// Dropping a UI leaves another UI's installed sender in place.
#[test]
fn dropping_a_foreign_ui_leaves_the_installed_sender_in_place() {
	let _slot = lock_active_ui();
	let (installed, mut receiver) = mpsc::unbounded_channel();
	let restore = active_ui().lock().expect("lock active test UI").replace(installed);

	{
		let (sender, foreign_receiver) = mpsc::unbounded_channel();
		let _foreign = ActiveTerminalUi {
			terminal: None,
			events: None,
			receiver: foreign_receiver,
			sender,
			app: test_app(),
			clipboard: None,
			copy_next_pairing_deeplink: false,
		};
	}

	let delivered = send_to_active(UiEvent::Log("after a foreign drop".to_string()));
	*active_ui().lock().expect("unlock active test UI") = restore;

	assert!(delivered, "the installed sender was cleared by a foreign drop");
	assert!(matches!(
		receiver.try_recv(),
		Ok(UiEvent::Log(line)) if line == "after a foreign drop"
	));
}

#[test]
fn sso_summary_bypasses_the_adjustable_log_filter() {
	let _slot = lock_active_ui();
	let (sender, mut receiver) = mpsc::unbounded_channel();
	let restore = active_ui().lock().expect("lock active test UI").replace(sender);
	let filtered_logs = tracing_subscriber::fmt::layer()
		.with_writer(io::sink)
		.with_filter(tracing_subscriber::EnvFilter::new("error"));
	let subscriber = tracing_subscriber::registry().with(SsoTranscriptLayer).with(filtered_logs);

	tracing::subscriber::with_default(subscriber, || {
		tracing::event!(
			target: SSO_TRANSCRIPT_TARGET,
			tracing::Level::INFO,
			cli_summary = "SSO response sent · get_account_alias · ok",
			cli_event = "response_sent",
			request = "get_account_alias",
			statement_request_id = "req:1",
			response_message_id = "msg:2",
			outcome = "ok",
			elapsed_ms = 84_u64,
		);
	});
	*active_ui().lock().expect("unlock active test UI") = restore;

	let UiEvent::Sso(event) = receiver.try_recv().expect("summary transcript event") else {
		panic!("expected SSO transcript event");
	};
	assert_eq!(event.kind.as_deref(), Some("response_sent"));
	assert_eq!(event.request.as_deref(), Some("get_account_alias"));
	assert_eq!(event.elapsed_ms, Some(84));
}

#[test]
fn system_events_share_copy_between_streaming_and_tui_renderers() {
	let event = SystemEvent::FramesListening { url: "ws://127.0.0.1:9956".to_string() };
	assert_eq!(event.human(), "• Listening for product frames\n  ws://127.0.0.1:9956");

	let mut app = test_app();
	app.handle_system_event(event);
	assert_eq!(app.transcript_text(), "• Listening for product frames\n  ws://127.0.0.1:9956");
}

/// §9 requires both arms of the dial report, and requires them where a
/// developer sees them. Both halves below run the one renderer: `human()`
/// builds an `App`, hands it the event, and returns that transcript. What
/// they pin is that each arm puts copy in the transcript at all, which is
/// what a report written as a `tracing` line would not do, since the
/// alternate screen covers stderr for the rest of the run.
#[test]
fn both_arms_of_the_debugger_report_render_in_either_mode() {
	let dialling = SystemEvent::DebuggerDialling {
		url: "ws://127.0.0.1:9231".to_string(),
		source: "--debugger".to_string(),
	};
	// The endpoint and the switch that supplied it both have to survive: the
	// source is the half that tells a developer which one to go and unset.
	assert!(dialling.human().contains("ws://127.0.0.1:9231"));
	assert!(dialling.human().contains("--debugger"));

	let mut app = test_app();
	app.handle_system_event(dialling);
	assert!(app.transcript_text().contains("ws://127.0.0.1:9231"));
	assert!(app.transcript_text().contains("--debugger"));

	// The off arm is the one silence would swallow, so it must say something.
	let off = SystemEvent::DebuggerOff;
	assert!(off.human().contains("Wire debugger off"));

	let mut app = test_app();
	app.handle_system_event(off);
	assert!(app.transcript_text().contains("Wire debugger off"));
}

#[test]
fn streaming_pairing_event_keeps_the_actionable_link() {
	let event =
		SystemEvent::PairingDeeplink { url: "polkadotapp://pair?handshake=0123".to_string() };

	assert!(event.human().contains("polkadotapp://pair?handshake=0123"));
}

#[test]
fn navigation_request_shows_the_destination() {
	let event = SystemEvent::NavigationRequested { url: "polkadot://dim2.dot/recover".to_string() };

	assert_eq!(event.human(), "• Navigation requested\n  polkadot://dim2.dot/recover");
}

#[test]
fn pairing_image_lifecycle_has_actionable_terminal_copy() {
	let mut app = test_app();
	app.handle_system_event(SystemEvent::PairingImagePasteReady);
	let ready = app.transcript_text();
	assert!(ready.contains("Waiting for a pairing QR image"));
	assert!(
		ready.contains(
			"Press Ctrl-V, use the terminal's paste shortcut, or drag an image file here"
		)
	);
	assert!(!ready.contains("http://"));

	app.handle_system_event(SystemEvent::PairingImageRead);

	assert!(app.transcript_text().contains("Pairing QR image received"));
}

#[test]
fn pairing_deeplink_renders_a_high_contrast_qr_without_copying_it() {
	let mut app = test_app();
	let url = format!("polkadotapp://pair?handshake={}", "0123456789abcdef".repeat(13));
	let rendered_side = QrCode::new(url.as_bytes()).expect("encode QR").width() + QR_QUIET_ZONE * 2;

	app.handle_system_event(SystemEvent::PairingDeeplink { url });

	let lines = app
		.entries
		.iter()
		.flat_map(|item| feed_item_lines(item, 120, usize::MAX))
		.collect::<Vec<_>>();
	let qr_style = Style::default().fg(Color::Black).bg(Color::White);
	let qr_rows = lines
		.iter()
		.filter_map(|line| line.spans.iter().find(|span| span.style == qr_style))
		.collect::<Vec<_>>();
	assert_eq!(qr_rows.len(), rendered_side.div_ceil(2));
	assert!(qr_rows.iter().all(|row| text_display_width(&row.content) == rendered_side));
	assert!(qr_rows.iter().any(|row| {
		row.content.chars().any(|character| matches!(character, '▀' | '▄' | '█'))
	}));
	assert!(!qr_rows.iter().any(|row| {
		row.content
			.chars()
			.any(|character| ('\u{2800}'..='\u{28ff}').contains(&character))
	}));
	assert!(qr_rows.windows(2).all(|rows| {
		text_display_width(&rows[0].content) == text_display_width(&rows[1].content)
	}));
	assert_eq!(
		app.transcript_text(),
		"◌ Pairing link ready\n  Open the dedicated link shown by the pairing host.\n• Pairing link\n  <pairing link>"
	);
}

#[test]
fn pairing_event_encodes_the_exact_deeplink() {
	let mut app = test_app();
	let url = "polkadotapp://pair?handshake=exact-payload".to_string();
	let expected = PairingQr::encode(&url).expect("encode expected QR code");

	app.handle_system_event(SystemEvent::PairingDeeplink { url });

	let actual = app.entries.iter().find_map(|item| match item {
		FeedItem::PairingQr(qr) => Some(qr),
		_ => None,
	});
	assert_eq!(actual, Some(&expected));
}

#[test]
fn solid_qr_cells_preserve_every_module() {
	let url = format!("polkadotapp://pair?handshake={}", "0123456789abcdef".repeat(13));
	let code = QrCode::new(url.as_bytes()).expect("encode QR");
	let side = code.width() + QR_QUIET_ZONE * 2;
	let dark = code.to_colors();
	let module = |row: usize, column: usize| {
		let (row, column) = (row.wrapping_sub(QR_QUIET_ZONE), column.wrapping_sub(QR_QUIET_ZONE));
		row < code.width() &&
			column < code.width() &&
			dark[row * code.width() + column] == QrColor::Dark
	};
	let qr = PairingQr::encode(&url).expect("encode pairing QR");
	let lines = pairing_qr_lines(&qr, 120, 120);

	assert_eq!(lines.len(), side.div_ceil(2));
	for (terminal_row, line) in lines.iter().enumerate() {
		let row = line.spans.last().expect("QR row span").content.as_ref();
		assert_eq!(row.chars().count(), side);
		for (column, character) in row.chars().enumerate() {
			let modules = match character {
				'█' => [true, true],
				'▀' => [true, false],
				'▄' => [false, true],
				' ' => [false, false],
				other => panic!("non-solid QR character {other:?}"),
			};
			assert_eq!(
				modules,
				[module(terminal_row * 2, column), module(terminal_row * 2 + 1, column)]
			);
		}
	}
}

#[test]
fn narrow_terminal_keeps_the_link_without_rendering_a_wrapped_qr() {
	let mut app = test_app();
	let url = format!("polkadotapp://pair?handshake={}", "0123456789abcdef".repeat(13));

	app.handle_system_event(SystemEvent::PairingDeeplink { url });

	let lines = app
		.entries
		.iter()
		.flat_map(|item| feed_item_lines(item, 36, usize::MAX))
		.collect::<Vec<_>>();
	let rendered = lines
		.iter()
		.flat_map(|line| line.spans.iter())
		.map(|span| span.content.as_ref())
		.collect::<String>();
	assert!(rendered.contains("QR code needs"));
	assert!(
		!lines
			.iter()
			.flat_map(|line| line.spans.iter())
			.any(|span| { span.style == Style::default().fg(Color::Black).bg(Color::White) })
	);
	assert!(app.transcript_text().contains("<pairing link>"));
}

#[test]
fn solid_pairing_qr_fits_an_eighty_column_forty_row_viewport() {
	let mut app = test_app();
	let url = format!("polkadotapp://pair?handshake={}", "0123456789abcdef".repeat(13));
	app.handle_system_event(SystemEvent::PairingDeeplink { url });

	let lines = app
		.entries
		.iter()
		.flat_map(|item| feed_item_lines(item, 80, 40))
		.collect::<Vec<_>>();

	assert!(
		lines
			.iter()
			.flat_map(|line| line.spans.iter())
			.any(|span| { span.style == Style::default().fg(Color::Black).bg(Color::White) })
	);
	assert!(
		!lines
			.iter()
			.flat_map(|line| line.spans.iter())
			.any(|span| span.content.contains("QR code needs"))
	);
}

#[test]
fn larger_pairing_qr_reports_the_solid_renderer_dimensions() {
	let oversized = PairingQr { rows: vec![" ".repeat(73); 37] };

	let lines = pairing_qr_lines(&oversized, 70, 36);
	let rendered = lines
		.iter()
		.flat_map(|line| line.spans.iter())
		.map(|span| span.content.as_ref())
		.collect::<String>();

	assert!(rendered.contains("QR code needs 75 columns × 37 rows"));
	assert!(
		!lines
			.iter()
			.flat_map(|line| line.spans.iter())
			.any(|span| { span.style == Style::default().fg(Color::Black).bg(Color::White) })
	);
}

#[test]
fn short_terminal_keeps_the_link_without_clipping_the_qr() -> Result<()> {
	let mut app = test_app();
	let url = format!("polkadotapp://pair?handshake={}", "0123456789abcdef".repeat(13));
	app.handle_system_event(SystemEvent::PairingDeeplink { url });

	let (screen, _) = render_app(&mut app, 120, 12)?;

	assert!(screen.contains("QR code needs"));
	assert!(!screen.chars().any(|character| matches!(character, '▀' | '▄' | '█')));
	assert!(app.transcript_text().contains("<pairing link>"));
	Ok(())
}

#[test]
fn pairing_qr_is_focused_completely_and_stays_anchored() -> Result<()> {
	let mut app = test_app();
	let url = format!("polkadotapp://pair?handshake={}", "0123456789abcdef".repeat(13));
	app.handle_system_event(SystemEvent::PairingDeeplink { url });
	let expected_qr_rows = app
		.entries
		.iter()
		.find_map(|item| match item {
			FeedItem::PairingQr(qr) => Some(qr.rendered_height()),
			_ => None,
		})
		.expect("pairing QR is retained");
	for index in 0..20 {
		app.stream(StreamKind::Stdout, format!("later log line {index}"));
	}

	let backend = TestBackend::new(120, 43);
	let mut terminal = Terminal::new(backend)?;
	terminal.draw(|frame| render(frame, &mut app))?;
	let visible_qr_rows = terminal
		.backend()
		.buffer()
		.content()
		.chunks(120)
		.filter(|row| row.iter().any(|cell| cell.style().bg == Some(Color::White)))
		.count();
	assert_eq!(visible_qr_rows, expected_qr_rows);
	let transcript_top = app.max_scroll_from_bottom - app.scroll_from_bottom;

	app.stream(StreamKind::Stdout, "new output while reading the QR".to_string());
	terminal.draw(|frame| render(frame, &mut app))?;
	assert_eq!(app.max_scroll_from_bottom - app.scroll_from_bottom, transcript_top);
	let visible_qr_rows = terminal
		.backend()
		.buffer()
		.content()
		.chunks(120)
		.filter(|row| row.iter().any(|cell| cell.style().bg == Some(Color::White)))
		.count();
	assert_eq!(visible_qr_rows, expected_qr_rows);
	Ok(())
}

#[test]
fn pairing_progress_returns_to_the_latest_output() {
	let mut app = test_app();
	app.handle_system_event(SystemEvent::PairingDeeplink {
		url: "polkadotapp://pair?handshake=0123".to_string(),
	});
	app.scroll_from_bottom = 10;

	app.handle_system_event(SystemEvent::PairingAuthenticating);

	assert!(!app.focus_pairing_qr);
	assert_eq!(app.scroll_from_bottom, 0);
}

#[test]
fn unencodable_pairing_deeplink_keeps_the_link_and_reports_the_qr_failure() {
	let mut app = test_app();
	let url = format!("polkadotapp://pair?handshake={}", "01".repeat(3_000));

	app.handle_system_event(SystemEvent::PairingDeeplink { url });

	let transcript = app.transcript_text();
	assert!(transcript.contains("<pairing link>"));
	assert!(transcript.contains("Could not render pairing QR code"));
}

#[test]
fn operator_login_copies_only_the_typed_pairing_deeplink_event() {
	let event = UiEvent::System(SystemEvent::PairingDeeplink {
		url: "polkadotapp://pair?handshake=0123".to_string(),
	});

	assert_eq!(pairing_deeplink_to_copy(true, &event), Some("polkadotapp://pair?handshake=0123"));
	assert_eq!(pairing_deeplink_to_copy(false, &event), None);
	assert_eq!(pairing_deeplink_to_copy(true, &UiEvent::Connection("pairing".to_string())), None);
}

#[test]
fn prompt_cursor_is_after_ascii_and_wide_input() -> Result<()> {
	let mut app = test_app();
	app.editor.set_text("/d");
	let (screen, cursor) = render_app(&mut app, 80, 12)?;
	assert!(
		screen
			.lines()
			.find(|line| line.contains("/d"))
			.is_some_and(|line| line.starts_with(" › /d"))
	);
	assert_eq!(cursor.x, 5);

	app.editor.set_text("/界");
	let (_, cursor) = render_app(&mut app, 80, 12)?;
	assert_eq!(cursor.x, 6);
	Ok(())
}

#[test]
fn long_input_viewport_keeps_cursor_visible() {
	let viewport = input_viewport("/script a/very/long/path.ts", 27, 10);
	assert!(viewport.text.starts_with('…'));
	assert!(viewport.cursor_x <= 10);
	assert!(viewport.text.ends_with("path.ts"));
}

#[test]
fn rendered_command_and_script_output_use_a_quiet_transcript() -> Result<()> {
	let mut app = test_app();
	app.push_command("/script demo.ts".to_string());
	app.handle_system_event(SystemEvent::ScriptStarted);
	app.stream(StreamKind::Stdout, "head follow event {".to_string());
	app.stream(StreamKind::Stdout, "  tag: Initialized".to_string());
	app.handle_system_event(SystemEvent::ScriptExit { code: 0 });

	let (screen, _) = render_app(&mut app, 80, 14)?;
	let divider = screen
		.lines()
		.find(|line| line.contains("/script demo.ts"))
		.expect("render command divider");
	assert!(divider.starts_with("─ /script demo.ts "));
	assert!(divider.ends_with('─'));
	assert!(screen.contains("  head follow event {"));
	assert!(screen.contains("✓ Script finished"));
	assert!(!screen.contains("SCRIPT ·"));
	assert!(!screen.contains("┌ command"));
	Ok(())
}

#[test]
fn script_lifecycle_replaces_running_label_with_outcome() -> Result<()> {
	let mut app = test_app();
	app.handle_system_event(SystemEvent::ScriptStarted);

	let (running, _) = render_app(&mut app, 80, 8)?;
	assert!(running.contains("◌ Script running"));

	app.handle_system_event(SystemEvent::ScriptExit { code: 0 });
	let (finished, _) = render_app(&mut app, 80, 8)?;
	assert!(finished.contains("✓ Script finished"));
	assert!(!finished.contains("Script running"));
	Ok(())
}

#[test]
fn renderer_stays_usable_at_narrow_normal_and_wide_sizes() -> Result<()> {
	for width in [40, 80, 120] {
		let mut app = test_app();
		app.push_command("/script scripts/a-long-product-script.ts".to_string());
		app.stream(
			StreamKind::Stdout,
			"finalized block 0x44119d48ae19d342a58828de9fce45f39bb".to_string(),
		);
		app.editor.set_text("/session production");

		let (screen, cursor) = render_app(&mut app, width, 12)?;

		assert!(screen.contains("👤"));
		assert!(screen.contains("🌐"));
		assert!(screen.contains("📦"));
		if width >= 80 {
			assert!(screen.contains("TrUAPI signing host"));
			assert!(screen.contains("disconnected"));
		}
		assert!(cursor.x < width);
	}
	Ok(())
}

#[test]
fn host_status_is_rendered_below_the_composer() -> Result<()> {
	let mut app = test_app();

	let (short, short_cursor) = render_app(&mut app, 80, 4)?;
	assert_eq!(short_cursor.y, 2);
	assert!(short.lines().nth(3).is_some_and(|line| line.contains("TrUAPI")));

	let (medium, medium_cursor) = render_app(&mut app, 80, 5)?;
	assert_eq!(medium_cursor.y, 3);
	assert!(medium.lines().nth(4).is_some_and(|line| line.contains("TrUAPI")));

	let (normal, normal_cursor) = render_app(&mut app, 80, 7)?;
	assert_eq!(normal_cursor.y, 4);
	assert_eq!(normal.lines().nth(5), Some(""));
	assert!(normal.lines().nth(6).is_some_and(|line| line.contains("TrUAPI")));
	Ok(())
}

#[test]
fn auto_follow_uses_ratatuis_word_wrapping() -> Result<()> {
	let mut app = test_app();
	for index in 0..8 {
		app.stream(
			StreamKind::Stdout,
			format!(
				"wrapped row {index} has several words that Ratatui moves between visual lines"
			),
		);
	}
	app.notice(NoticeTone::Success, "Newest result".to_string(), None);

	let (screen, _) = render_app(&mut app, 40, 10)?;
	assert!(screen.contains("Newest result"));
	Ok(())
}

#[test]
fn approval_text_is_sanitized_before_storage() {
	let mut app = test_app();
	let (response, _answer) = oneshot::channel();
	app.handle_event(UiEvent::Approval {
		action: "\u{1b}[31msign request\u{1b}[0m".to_string(),
		detail: "\u{1b}]0;unsafe title\u{7}safe detail".to_string(),
		kind: ApprovalKind::Action,
		response,
	});

	let transcript = app.transcript_text();
	assert!(transcript.contains("sign request"));
	assert!(transcript.contains("safe detail"));
	assert!(!transcript.contains('\u{1b}'));
}

#[test]
fn sso_request_and_response_collapse_to_one_human_row() {
	let mut app = test_app();
	app.handle_sso_event(SsoEvent {
		kind: Some("request_received".to_string()),
		request: Some("get_account_alias".to_string()),
		statement_request_id: Some("req:1".to_string()),
		remote_message_id: Some("msg:1".to_string()),
		..SsoEvent::default()
	});
	app.handle_sso_event(SsoEvent {
		kind: Some("response_sent".to_string()),
		request: Some("get_account_alias".to_string()),
		statement_request_id: Some("req:1".to_string()),
		response_message_id: Some("msg:2".to_string()),
		elapsed_ms: Some(84),
		..SsoEvent::default()
	});

	assert_eq!(app.entries.len(), 1);
	let transcript = app.transcript_text();
	assert!(transcript.contains("✓ Get account alias · 84 ms"));
	assert!(transcript.contains("request req:1 · message msg:2"));
}

#[test]
fn streaming_sso_summary_keeps_ids_as_one_detail_line() {
	let text = sso_event_text(SsoEvent {
		kind: Some("response_sent".to_string()),
		request: Some("resource_allocation".to_string()),
		statement_request_id: Some("yiBKUPOF".to_string()),
		response_message_id: Some("yiBKUPOF:response".to_string()),
		outcome: Some("ok".to_string()),
		elapsed_ms: Some(901),
		fallback_summary: Some("unused fallback".to_string()),
		..SsoEvent::default()
	});

	assert_eq!(
		text,
		"✓ Resource allocation · 901 ms\n  request yiBKUPOF · message yiBKUPOF:response"
	);
	assert!(!text.contains("statement_request_id="));
	assert!(!text.contains("responding_to="));
}

#[test]
fn rejected_resource_allocation_is_visibly_failed_with_its_reason() {
	let mut app = test_app();
	app.handle_sso_event(SsoEvent {
		kind: Some("request_received".to_string()),
		request: Some("resource_allocation".to_string()),
		statement_request_id: Some("UaLEyWid".to_string()),
		remote_message_id: Some("UaLEyWid".to_string()),
		..SsoEvent::default()
	});
	app.handle_sso_event(SsoEvent {
		kind: Some("response_sent".to_string()),
		request: Some("resource_allocation".to_string()),
		statement_request_id: Some("UaLEyWid".to_string()),
		response_message_id: Some("UaLEyWid:response".to_string()),
		outcome: Some("rejected".to_string()),
		elapsed_ms: Some(63_583),
		reason: Some(
			"Requested resource was rejected: timed out waiting for Bulletin authorization"
				.to_string(),
		),
		..SsoEvent::default()
	});

	let transcript = app.transcript_text();
	assert!(transcript.contains("× Resource allocation rejected · 63583 ms"));
	assert!(
		transcript.contains(
			"Requested resource was rejected: timed out waiting for Bulletin authorization"
		)
	);
	assert!(transcript.contains("request UaLEyWid · message UaLEyWid:response"));
	assert!(!transcript.contains("✓ Resource allocation"));
}

#[test]
fn partial_and_unavailable_sso_results_use_warning_presentation() {
	assert_eq!(
		sso_activity_presentation(
			"Resource allocation".to_string(),
			"response_sent",
			Some("partial")
		),
		("Resource allocation partially completed".to_string(), ActivityState::Warning)
	);
	assert_eq!(
		sso_activity_presentation(
			"Resource allocation".to_string(),
			"response_sent",
			Some("not_available")
		),
		("Resource allocation unavailable".to_string(), ActivityState::Warning)
	);
}

#[test]
fn child_output_sanitizer_removes_terminal_control_sequences() {
	assert_eq!(sanitize_terminal_text("\u{1b}[31mred\u{1b}[0m\u{1b}]0;title\u{7}"), "red");
}

#[test]
fn composer_surface_blends_for_dark_and_light_terminals() {
	assert_eq!(blended_surface((0, 0, 0)), (30, 30, 30));
	assert_eq!(blended_surface((255, 255, 255)), (244, 244, 244));
	assert_eq!(ansi256_to_rgb(16), Some((0, 0, 0)));
	assert_eq!(ansi256_to_rgb(231), Some((255, 255, 255)));
	assert_eq!(rgb_to_ansi256(255, 255, 255), 231);
}

#[test]
fn forced_color_styles_redirected_output_but_never_overrides_no_color() {
	assert!(should_style_output(true, false, false));
	assert!(should_style_output(false, false, true));
	assert!(!should_style_output(false, false, false));
	assert!(!should_style_output(true, true, true));
}

fn render_app(app: &mut App, width: u16, height: u16) -> Result<(String, Position)> {
	let backend = TestBackend::new(width, height);
	let mut terminal = Terminal::new(backend)?;
	terminal.draw(|frame| render(frame, app))?;
	let cursor = terminal.backend().cursor_position();
	let screen = terminal
		.backend()
		.buffer()
		.content()
		.chunks(usize::from(width))
		.map(|row| {
			row.iter()
				.map(|cell| cell.symbol())
				.collect::<Vec<_>>()
				.join("")
				.trim_end()
				.to_string()
		})
		.collect::<Vec<_>>()
		.join("\n");
	Ok((screen, cursor))
}

fn line_text(line: Line<'_>) -> String {
	line.spans.iter().map(|span| span.content.as_ref()).collect()
}
