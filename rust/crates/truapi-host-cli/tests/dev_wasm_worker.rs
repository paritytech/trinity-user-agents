//! `truapi-host dev --worker-wasm` runs an example worker of `rust/guests` as
//! the `Worker` of its product. The tests act as the person on the chat socket
//! `/chat/ws`. With `chat-actions`, a room that the host stores: they read the
//! rooms and the messages, type a message, press a button and run a command,
//! and read the answers of the worker. With `chat-source`, a room that the
//! worker keeps: they read a page of it and type a message, which the host asks
//! of the worker.

use std::{
	io::{BufRead, BufReader},
	net::TcpListener,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	sync::mpsc,
	time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// A testnet mnemonic. The host signs only on loopback for this test.
const MNEMONIC: &str =
	"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

const PRODUCT_ID: &str = "chat-actions.dot";

const ROOM: &str = "actions";

/// Build the example worker `name` of `rust/guests`, and give its path.
fn guest(name: &str) -> PathBuf {
	let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../guests/Cargo.toml");
	let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guests");
	let status = Command::new(env!("CARGO"))
		.args(["build", "--release", "--target", "wasm32-unknown-unknown"])
		.args(["-p", &format!("{name}-guest")])
		.arg("--manifest-path")
		.arg(manifest)
		.arg("--target-dir")
		.arg(&target_dir)
		.status()
		.expect("cargo runs");
	assert!(status.success(), "the guest worker failed to build");
	target_dir.join(format!("wasm32-unknown-unknown/release/{}_guest.wasm", name.replace('-', "_")))
}

fn free_port() -> u16 {
	TcpListener::bind("127.0.0.1:0")
		.and_then(|listener| listener.local_addr())
		.unwrap()
		.port()
}

/// The host process, stopped when the test ends.
struct Host {
	child: Child,
	lines: mpsc::Receiver<String>,
	seen: Vec<String>,
}

impl Host {
	fn start(base_path: &Path, port: u16, product_id: &str, wasm: &Path) -> Self {
		let mut child = Command::new(env!("CARGO_BIN_EXE_truapi-host"))
			.args(["dev", "--port", &port.to_string(), "--product-id", product_id])
			.arg("--worker-wasm")
			.arg(wasm)
			.arg("--base-path")
			.arg(base_path)
			.args(["--mnemonic", MNEMONIC])
			.env_remove("TRUAPI_CHAT_LOG")
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.expect("truapi-host starts");
		let (sender, lines) = mpsc::channel();
		for stream in [
			Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
			Box::new(child.stderr.take().unwrap()),
		] {
			let sender = sender.clone();
			std::thread::spawn(move || {
				for line in BufReader::new(stream).lines().map_while(Result::ok) {
					let _ = sender.send(line);
				}
			});
		}
		Self { child, lines, seen: Vec::new() }
	}

	/// Wait for an output line that contains `text`.
	fn wait_for_line(&mut self, text: &str, timeout: Duration) -> String {
		if let Some(line) = self.seen.iter().find(|line| line.contains(text)) {
			return line.clone();
		}
		let deadline = Instant::now() + timeout;
		while let Some(left) = deadline.checked_duration_since(Instant::now()) {
			match self.lines.recv_timeout(left) {
				Ok(line) => {
					self.seen.push(line.clone());
					if line.contains(text) {
						return line;
					}
				},
				Err(_) => break,
			}
		}
		panic!("no output line contains {text:?}; output:\n{}", self.seen.join("\n"));
	}
}

impl Drop for Host {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

type Socket =
	tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The person on the chat socket, with the frames the host sent.
struct Person {
	socket: Socket,
	frames: Vec<Value>,
}

impl Person {
	async fn connect(port: u16) -> Self {
		let (socket, _) = connect_async(format!("ws://127.0.0.1:{port}/chat/ws"))
			.await
			.expect("the chat socket opens");
		Self { socket, frames: Vec::new() }
	}

	async fn send(&mut self, op: Value) {
		self.socket.send(Message::Text(op.to_string())).await.expect("op is sent");
	}

	/// Read frames until one matches `matches`, and give it.
	async fn until(&mut self, what: &str, matches: impl Fn(&Value) -> bool) -> Value {
		if let Some(frame) = self.frames.iter().find(|frame| matches(frame)) {
			return frame.clone();
		}
		let read = async {
			while let Some(message) = self.socket.next().await {
				let Ok(Message::Text(text)) = message else {
					continue;
				};
				let frame: Value = serde_json::from_str(&text).expect("frames are JSON");
				self.frames.push(frame.clone());
				if matches(&frame) {
					return Some(frame);
				}
			}
			None
		};
		match tokio::time::timeout(Duration::from_secs(30), read).await {
			Ok(Some(frame)) => frame,
			_ => panic!("no frame for {what}; frames:\n{:#}", Value::from(self.frames.clone())),
		}
	}

	/// Wait for a message of the worker in the room with the text `text`.
	async fn product_message(&mut self, text: &str) -> Value {
		self.until(text, |frame| message_in_room(frame, "product", text)).await["message"].clone()
	}
}

/// Whether `frame` is a change to the message of `origin` with the text
/// `text` in the room.
fn message_in_room(frame: &Value, origin: &str, text: &str) -> bool {
	let is = |message: &Value| message["origin"] == origin && message["body"]["text"] == text;
	match frame["kind"].as_str() {
		Some("message") => frame["roomId"] == ROOM && is(&frame["message"]),
		_ => false,
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_acts_in_the_rooms_of_a_wasm_worker_on_the_chat_socket() {
	let wasm = guest("chat-actions");
	let state = tempfile::tempdir().unwrap();
	let port = free_port();
	let mut host = Host::start(state.path(), port, PRODUCT_ID, &wasm);
	host.wait_for_line("Wasm worker started", Duration::from_secs(120));
	let ready = host.wait_for_line("[worker] room actions ready", Duration::from_secs(60));
	let menu_id = ready.rsplit(' ').next().unwrap().to_string();

	let mut person = Person::connect(port).await;
	let snapshot = person.until("the snapshot", |frame| frame["kind"] == "snapshot").await;
	assert_eq!(snapshot["productId"], PRODUCT_ID);
	assert_eq!(snapshot["worker"], json!({ "configured": true, "connected": 1 }));
	let room = snapshot["rooms"]
		.as_array()
		.unwrap()
		.iter()
		.find(|room| room["roomId"] == ROOM)
		.cloned()
		.expect("the worker created its room");
	assert_eq!(room["name"], "Actions");
	assert_eq!(room["commands"][0]["name"], "ping");
	let menu = snapshot["messages"]
		.as_array()
		.unwrap()
		.iter()
		.find(|entry| entry["roomId"] == ROOM && entry["message"]["messageId"] == menu_id)
		.map(|entry| entry["message"].clone())
		.expect("the menu is in the snapshot");
	assert_eq!(menu["body"]["type"], "Actions");
	assert_eq!(menu["body"]["actions"][0], json!({ "actionId": "yes", "title": "YES" }));

	person
		.send(json!({ "op": "post", "reqId": 1, "roomId": ROOM, "text": "hello" }))
		.await;
	let answer = person.until("the post answer", |frame| frame["reqId"] == 1).await;
	assert_eq!(answer["ok"]["type"], "Posted");
	assert_eq!(answer["ok"]["workers"], 1);
	person.product_message("echo: hello").await;
	host.wait_for_line("[worker] echo: hello", Duration::from_secs(10));

	person
		.send(json!({ "op": "trigger", "roomId": ROOM, "messageId": menu_id, "actionId": "yes" }))
		.await;
	let delivered = person.until("the press", |frame| frame["op"] == "trigger").await;
	assert_eq!(delivered, json!({ "kind": "delivered", "op": "trigger", "workers": 1 }));
	person.product_message(&format!("pressed yes on {menu_id}")).await;

	person
		.send(json!({ "op": "command", "roomId": ROOM, "command": "ping", "payload": "a b" }))
		.await;
	person.product_message("command /ping a b").await;

	// A page that opens later sees the whole room: the host keeps it.
	let mut later = Person::connect(port).await;
	let snapshot = later.until("the second snapshot", |frame| frame["kind"] == "snapshot").await;
	let texts: Vec<_> = snapshot["messages"]
		.as_array()
		.unwrap()
		.iter()
		.filter(|entry| entry["roomId"] == ROOM)
		.map(|entry| entry["message"]["body"]["text"].as_str().unwrap_or_default().to_string())
		.collect();
	assert_eq!(
		texts,
		[
			"Press a button".to_string(),
			"hello".to_string(),
			"echo: hello".to_string(),
			format!("pressed yes on {menu_id}"),
			"command /ping a b".to_string(),
		]
	);
}

/// The id and the text of each message of a page answer, as the page reads
/// them.
fn page_texts(answer: &Value) -> Vec<(String, String)> {
	answer["ok"]["messages"]
		.as_array()
		.expect("the answer is a page")
		.iter()
		.map(|message| {
			let body = &message["body"];
			let text = body["text"].as_str().or(body["previewText"].as_str()).unwrap_or_default();
			(message["messageId"].as_str().unwrap().to_string(), text.to_string())
		})
		.collect()
}

/// A room that a wasm worker keeps works on the chat socket as one that a
/// script worker keeps: the host asks the worker for a page and for each
/// message the person types, and shows what the worker answers.
#[tokio::test(flavor = "multi_thread")]
async fn a_person_reads_and_writes_a_room_that_a_wasm_worker_keeps() {
	let wasm = guest("chat-source");
	let state = tempfile::tempdir().unwrap();
	let port = free_port();
	let mut host = Host::start(state.path(), port, "chat-source.dot", &wasm);
	host.wait_for_line("[worker] room notes ready", Duration::from_secs(120));
	let mut person = Person::connect(port).await;
	let snapshot = person.until("the snapshot", |frame| frame["kind"] == "snapshot").await;
	let room = snapshot["rooms"]
		.as_array()
		.unwrap()
		.iter()
		.find(|room| room["roomId"] == "notes");
	assert_eq!(room.map(|room| &room["name"]), Some(&json!("Notes")));
	let page = |req_id: u64| {
		json!({
			"op": "messages", "reqId": req_id, "roomId": "notes",
			"before": true, "cursor": null, "pageSize": 20,
		})
	};

	person.send(page(1)).await;
	let first = person.until("the first page", |frame| frame["reqId"] == 1).await;
	person
		.send(json!({ "op": "post", "reqId": 2, "roomId": "notes", "text": "hello" }))
		.await;
	let posted = person.until("the post answer", |frame| frame["reqId"] == 2).await;
	host.wait_for_line("[worker] stored n1: hello", Duration::from_secs(10));
	person.send(page(3)).await;
	let second = person.until("the second page", |frame| frame["reqId"] == 3).await;

	assert_eq!(page_texts(&first), [("n0".to_string(), "Welcome card".to_string())]);
	assert_eq!(
		(&posted["ok"]["type"], &posted["ok"]["message"]["messageId"]),
		(&json!("MessageCreated"), &json!("n1"))
	);
	assert_eq!(
		page_texts(&second),
		[("n0", "Welcome card"), ("n1", "hello"), ("n2", "noted: hello")]
			.map(|(id, text)| (id.to_string(), text.to_string()))
	);
}
