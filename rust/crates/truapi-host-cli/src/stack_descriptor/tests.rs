use trinity_stack::DEFAULT_LINK_BASE;

use super::*;

#[test]
fn a_key_that_contradicts_the_link_is_refused() {
	let key = format!("0x{}", hex::encode(trinity_stack::public_key(&[7; 32])));
	let other = format!("0x{}", hex::encode(trinity_stack::public_key(&[9; 32])));
	let link = Pointer::new("https://a.example/s.json", Some(&key))
		.unwrap()
		.to_link(DEFAULT_LINK_BASE);
	assert!(pointer_of(&link, Some(&other)).is_err());
	assert_eq!(
		pointer_of(&link, Some(&key)).unwrap().unwrap().key(),
		Pointer::from_link(&link).unwrap().key()
	);
	let unkeyed = Pointer::new("https://a.example/s.json", None)
		.unwrap()
		.to_link(DEFAULT_LINK_BASE);
	let filled = pointer_of(&unkeyed, Some(&key)).unwrap().unwrap();
	assert_eq!(filled.key().map(hex::encode), Some(key[2..].to_owned()));
	assert!(pointer_of("./stack.json", None).unwrap().is_none());
}

#[test]
fn a_link_or_url_breaking_a_rule_says_which_instead_of_reading_a_file() {
	let code = |stack: &str| pointer_of(stack, None).unwrap_err().to_string();
	let plain_http = "polkadotapp://stack?url=http%3A%2F%2Fstack.example%2Fs.json";
	assert!(code(plain_http).contains("invalid-url"), "{}", code(plain_http));
	let short_key = "polkadotapp://stack?key=0x12&url=https%3A%2F%2Fstack.example%2Fs.json";
	assert!(code(short_key).contains("invalid-hex"), "{}", code(short_key));
	assert!(code("http://stack.example/s.json").contains("invalid-url"));
	let app_link = "https://app.example/open?url=https%3A%2F%2Fstack.example%2Fs.json";
	assert_eq!(
		pointer_of(app_link, None).unwrap().unwrap().url().as_str(),
		"https://stack.example/s.json"
	);
}

/// A server that answers with an error status is told apart from one that
/// does not answer, so a moved descriptor can be reported louder.
#[tokio::test]
async fn test_fetch_http_status_error() {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("http://{}/stack.json", listener.local_addr().unwrap());
	tokio::spawn(async move {
		let (mut socket, _) = listener.accept().await.unwrap();
		let mut request = [0; 1024];
		let _ = socket.read(&mut request).await;
		let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
		socket.write_all(response.as_bytes()).await.unwrap();
	});
	let error = fetch(&url, FETCH_TIMEOUT).await.expect_err("404 accepted");
	assert!(matches!(&error, FetchError::Http(status) if status.contains("404")));

	let closed_port_error = fetch("http://127.0.0.1:9/stack.json", FETCH_TIMEOUT)
		.await
		.expect_err("closed port accepted");
	assert!(matches!(closed_port_error, FetchError::Transport(_)));
}

/// A descriptor with every role, a `dub` service and a `popController`, edited.
fn stack_descriptor(edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	let chain = |role: &str, byte: &str, para_id: Option<u32>| {
		let mut chain = serde_json::json!({
			"role": role,
			"genesisHash": format!("0x{}", byte.repeat(32)),
			"rpc": [format!("ws://127.0.0.1:99{byte}")],
		});
		if let Some(para_id) = para_id {
			chain["paraId"] = para_id.into();
		}
		chain
	};
	let mut document = serde_json::json!({
		"$v": 1, "sequence": 1, "generation": "g1", "id": "local", "name": "Local",
		"network": "local", "tld": "paseo",
		"services": [{ "kind": "dub", "urls": ["http://127.0.0.1:8080/"] }],
		"dotns": { "popController": format!("0x{}", "ab".repeat(20)) },
		"chains": [
			chain("Relay", "01", None),
			chain("People", "02", Some(1004)),
			chain("AssetHub", "03", Some(1000)),
			chain("Bulletin", "04", Some(1010)),
		],
	});
	edit(&mut document);
	Descriptor::from_json(document).unwrap()
}

/// A `lightSpec` whose `sha256` repeats `byte`.
fn light_spec(byte: &str) -> serde_json::Value {
	let sha256 = format!("0x{}", byte.repeat(32));
	serde_json::json!({ "url": "https://spec.example/light.json", "sha256": sha256 })
}

/// An edit to a descriptor's JSON.
type DescriptorEdit = Box<dyn FnOnce(&mut serde_json::Value)>;

/// The runtime notice and `stack bundle --skip-same-projection` read one list
/// of fields: a change to any of them is news, and a change elsewhere is not.
/// The light specs are news only to the bundle, which ships them: a relay's
/// rotates with every fresh checkpoint, and a notice on every command for it
/// would tell the user nothing they need to act on.
#[test]
fn test_reads_same_by_field() {
	let base = stack_descriptor(|_| {});
	let people = |edit: fn(&mut serde_json::Value)| {
		move |descriptor: &mut serde_json::Value| edit(&mut descriptor["chains"][1])
	};
	let differs: [(&str, DescriptorEdit); 9] = [
		("id", Box::new(|descriptor| descriptor["id"] = "other".into())),
		("tld", Box::new(|descriptor| descriptor["tld"] = "testnet".into())),
		("generation", Box::new(|descriptor| descriptor["generation"] = "g2".into())),
		(
			"dub",
			Box::new(|descriptor| {
				descriptor["services"][0]["urls"] = serde_json::json!(["http://127.0.0.1:8081/"]);
			}),
		),
		(
			"popController",
			Box::new(|descriptor| {
				descriptor["dotns"]["popController"] = format!("0x{}", "cd".repeat(20)).into();
			}),
		),
		(
			"contracts",
			Box::new(|descriptor| {
				descriptor["contracts"] =
					serde_json::json!({ "accountDataStore": format!("0x{}", "22".repeat(20)) });
			}),
		),
		(
			"role genesis",
			Box::new(people(|chain| {
				chain["genesisHash"] = format!("0x{}", "05".repeat(32)).into()
			})),
		),
		(
			"role rpc",
			Box::new(people(|chain| {
				chain["rpc"].as_array_mut().unwrap().push("ws://127.0.0.1:9999".into());
			})),
		),
		(
			"role first rpc",
			Box::new(people(|chain| chain["rpc"] = serde_json::json!(["ws://127.0.0.1:9998"]))),
		),
	];
	let light_specs: [(&str, DescriptorEdit); 2] = [
		("role light spec", Box::new(people(|chain| chain["lightSpec"] = light_spec("06")))),
		(
			"relay light spec",
			Box::new(|descriptor| descriptor["chains"][0]["lightSpec"] = light_spec("07")),
		),
	];
	let noticed = |edits: Vec<(&'static str, DescriptorEdit)>| {
		edits
			.into_iter()
			.map(|(field, edit)| {
				let edited = stack_descriptor(edit);
				(field, !reads_same(&base, &edited), !bundles_same(&base, &edited))
			})
			.collect::<Vec<_>>()
	};
	assert_eq!(
		noticed(differs.into_iter().collect()),
		[
			("id", true, true),
			("tld", true, true),
			("generation", true, true),
			("dub", true, true),
			("popController", true, true),
			("contracts", true, true),
			("role genesis", true, true),
			("role rpc", true, true),
			("role first rpc", true, true),
		]
	);
	assert_eq!(
		noticed(light_specs.into_iter().collect()),
		[("role light spec", false, true), ("relay light spec", false, true)]
	);

	let renamed = stack_descriptor(|descriptor| {
		descriptor["name"] = "Renamed".into();
		descriptor["sequence"] = 2.into();
	});
	assert!(bundles_same(&base, &renamed));
}

/// A source and its fork can both be local files with one descriptor `id`. If
/// every local file were one stack, the CLI would keep their state in one
/// directory and the fork would read the source's session.
#[test]
fn two_local_files_are_two_stacks_and_one_file_is_one() {
	let directory = tempfile::tempdir().expect("tempdir");
	let write = |name: &str| {
		let path = directory.path().join(name);
		let bytes = serde_json::to_vec(&stack_descriptor(|_| {})).expect("descriptor is JSON");
		std::fs::write(&path, bytes).expect("descriptor file is written");
		path
	};
	let (source, fork) = (write("source.json"), write("fork.json"));
	let respelled = directory.path().join(".").join("source.json");
	let id = |path: std::path::PathBuf| {
		let path = path.to_str().expect("temporary path is UTF-8").to_owned();
		stack_id(&path, None).expect("a local file has an id")
	};

	let (source_id, fork_id, respelled_id) = (id(source), id(fork), id(respelled));
	assert_eq!((source_id == fork_id, source_id == respelled_id), (false, true));
}

/// A lossy conversion turns every byte that is not UTF-8 into one replacement
/// character, so two files whose paths differ only there would be one stack
/// and share their state. Such a path is refused instead, with what to do
/// about it.
#[cfg(unix)]
#[test]
fn a_local_file_path_that_is_not_utf8_is_refused() {
	use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

	let path_with = |byte: u8| PathBuf::from(OsString::from_vec(vec![b'/', byte, b'.', b'j']));
	let ids = [path_with(0xfe), path_with(0xff), PathBuf::from("/s.json")]
		.map(|path| canonical_file_id(path).map_err(|error| error.to_string()));
	let refusal = "/\u{fffd}.j is not a UTF-8 path, so it cannot name a stack; copy the file to \
	               a path that is UTF-8";
	assert_eq!(
		ids,
		[Err(refusal.to_string()), Err(refusal.to_string()), Ok("file:/s.json".to_string())]
	);
}

/// A fifo's path can carry any descriptor, so naming the stack by it would put
/// every descriptor sent through it in one state directory. It is refused,
/// with what to do instead. Its id is asked for without reading it, so the
/// test does not wait for a writer.
#[cfg(unix)]
#[test]
fn a_local_stack_that_is_not_a_regular_file_is_refused() {
	let directory = tempfile::tempdir().expect("tempdir");
	let fifo = directory.path().join("stack.fifo");
	let created = std::process::Command::new("mkfifo").arg(&fifo).status().expect("run mkfifo");
	assert!(created.success(), "mkfifo {}", fifo.display());
	let path = fifo.to_str().expect("temporary path is UTF-8");

	assert_eq!(
		stack_id(path, None).map_err(|error| error.to_string()),
		Err(format!(
			"{path} is not a regular file, so it cannot name a stack; save the descriptor to a \
			 file and give its path"
		))
	);
}

/// A regular file opened as `/dev/fd/<n>`, as `--stack /dev/stdin < s.json`
/// gives, resolves on macOS to `/dev/fd/<file name>`, which every file of that
/// name shares, so it is refused there. Linux resolves it to the file itself,
/// which names the stack as the file's own path does.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_local_stack_opened_as_a_file_descriptor_is_named_by_its_file_or_refused() {
	use std::os::fd::AsRawFd as _;

	let file = tempfile::NamedTempFile::new().expect("temporary file");
	let path = format!("/dev/fd/{}", file.as_file().as_raw_fd());
	let name = file.path().file_name().expect("a file name").to_str().expect("UTF-8");
	let canonical = std::fs::canonicalize(file.path()).expect("the file resolves");

	let expected = if cfg!(target_os = "macos") {
		Err(format!(
			"{path} resolves to /dev/fd/{name}, a file descriptor's path that another file can \
			 resolve to as well, so it cannot name a stack; save the descriptor to a file and give \
			 its path"
		))
	} else {
		Ok(format!("file:{}", canonical.display()))
	};
	assert_eq!(stack_id(&path, None).map_err(|error| error.to_string()), expected);
}
