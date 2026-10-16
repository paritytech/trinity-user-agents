//! Conformance vectors: generated here, committed under `vectors/`, and run by every reader.
//!
//! `TRINITY_STACK_WRITE_VECTORS=1 cargo test -p trinity-stack --test vectors` rewrites them after
//! a deliberate change. Without it, the test fails when the committed files differ from what this
//! generator produces, so they cannot drift from the cases below.

use base64::{Engine, engine::general_purpose::STANDARD};
use curve25519_dalek::{
	constants::{ED25519_BASEPOINT_POINT, EIGHT_TORSION},
	scalar::Scalar,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use std::{collections::BTreeMap, fs, path::PathBuf};
use trinity_stack::{Fetched, Pointer, accept, open, public_key, sign};

const SECRET: [u8; 32] = [7; 32];
const OTHER_SECRET: [u8; 32] = [9; 32];
const URL: &str = "https://previewnet.substrate.dev/.well-known/trinity-stack.json";
/// 2026-10-15T12:00:00Z, the time every reader case is judged at.
const NOW: i64 = 1_792_065_600;
/// Times relative to `NOW`, in Unix seconds.
const TWO_HOURS_AGO: i64 = NOW - 7_200;
const ONE_HOUR_AGO: i64 = NOW - 3_600;
const YESTERDAY: i64 = NOW - 86_400;

/// Previewnet's genesis hashes, as the provider catalog has them.
const RELAY: &str = "0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92";
const ASSET_HUB: &str = "0xbac97e23fc8f4bccae72a98f8aeb2bcab20bf755862304e4b46ad6473456e896";
const PEOPLE: &str = "0x55e3e689ecfa9d2fffcf7d309b8011956671493982230bfd0420c683542249e9";

fn hex(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

fn key() -> String {
	hex(&public_key(&SECRET))
}

fn base() -> Value {
	json!({
		"$v": 1,
		"sequence": 42,
		"generation": "2026-09-17T16:15:00Z",
		"id": "previewnet",
		"name": "Previewnet",
		"network": "paseo",
		"tld": "testnet",
		"chains": [
			{ "role": "Relay", "genesisHash": RELAY, "rpc": ["wss://previewnet.substrate.dev/relay"] },
			{
				"role": "AssetHub",
				"genesisHash": ASSET_HUB,
				"paraId": 1000,
				"rpc": ["wss://previewnet.substrate.dev/asset-hub"],
				"lightSpec": {
					"url": "https://previewnet.substrate.dev/chainspecs/asset-hub.smol.json",
					"sha256": hex(&[0x11; 32]),
				},
			},
			{
				"role": "People",
				"genesisHash": PEOPLE,
				"paraId": 1004,
				"rpc": ["wss://previewnet.substrate.dev/people"],
				"statementStore": true,
			},
		],
		"services": [
			{ "kind": "dub", "urls": ["https://previewnet.substrate.dev/dub/"] },
			{ "kind": "ipfs-gateway", "urls": ["https://previewnet.substrate.dev/ipfs/"] },
			{ "kind": "eth-rpc", "urls": ["https://previewnet.substrate.dev/eth-rpc"], "chainId": 420420421 },
			{ "kind": "stun", "urls": ["stun:previewnet.substrate.dev:3478"] },
		],
		"assets": [{ "role": "payment", "chain": "AssetHub", "pallet": "Assets", "id": 1984 }],
		"contracts": { "accountDataStore": hex(&[0x22; 20]) },
		"coinage": { "chain": "People", "instance": 0 },
	})
}

fn with(edit: impl FnOnce(&mut Value)) -> Value {
	let mut value = base();
	edit(&mut value);
	value
}

fn bytes(value: &Value) -> Vec<u8> {
	serde_json::to_vec_pretty(value).unwrap()
}

fn envelope(version: u64, payload: &[u8], signature: &[u8]) -> Vec<u8> {
	bytes(
		&json!({ "$v": version, "payload": STANDARD.encode(payload), "signature": hex(signature) }),
	)
}

fn signed(value: &Value) -> Vec<u8> {
	sign(&bytes(value), &SECRET).unwrap()
}

/// A fork of the stack `SECRET` signs, as its spawner would publish it.
fn fork() -> Value {
	with(|value| {
		value["fork"] = json!({ "of": { "url": URL, "key": key() }, "at": TWO_HOURS_AGO });
		for (index, chain) in value["chains"].as_array_mut().unwrap().iter_mut().enumerate() {
			chain["forkedAt"] = json!({ "number": 1735 + index, "hash": hex(&[index as u8; 32]) });
		}
	})
}

/// A valid signature by `SECRET` under the `v<version>` prefix, bypassing `sign`'s checks so a
/// vector can break exactly one other rule.
fn signature_over(version: u64, payload: &[u8]) -> [u8; 64] {
	use ed25519_dalek::{Signer, SigningKey};
	let mut message = format!("trinity-stack/descriptor/v{version}").into_bytes();
	message.extend_from_slice(payload);
	SigningKey::from_bytes(&SECRET).sign(&message).to_bytes()
}

/// An envelope a reader can only refuse for its payload: the signature is valid.
fn envelope_signed(version: u64, payload: &[u8]) -> Vec<u8> {
	envelope(version, payload, &signature_over(version, payload))
}

/// The parts of a valid envelope over `base()`, for tampering with.
fn signature_parts() -> (Vec<u8>, [u8; 64]) {
	let document: Value = serde_json::from_slice(&signed(&base())).unwrap();
	let payload = STANDARD.decode(document["payload"].as_str().unwrap()).unwrap();
	let signature = hex::decode(&document["signature"].as_str().unwrap()[2..]).unwrap();
	(payload, signature.try_into().unwrap())
}

/// `SECRET`'s scalar `a`, as Ed25519 derives it.
fn secret_scalar() -> Scalar {
	let expanded = Sha512::digest(SECRET);
	let mut clamped: [u8; 32] = expanded[..32].try_into().unwrap();
	clamped[0] &= 248;
	clamped[31] &= 127;
	clamped[31] |= 64;
	Scalar::from_bytes_mod_order(clamped)
}

/// The challenge `k = H(R || A || prefix || payload)`.
fn challenge(r: &[u8; 32], key: &[u8; 32], payload: &[u8]) -> Scalar {
	Scalar::from_hash(
		Sha512::new()
			.chain_update(r)
			.chain_update(key)
			.chain_update(b"trinity-stack/descriptor/v1")
			.chain_update(payload),
	)
}

fn signature_of(r: &[u8; 32], s: Scalar) -> [u8; 64] {
	let mut signature = [0; 64];
	signature[..32].copy_from_slice(r);
	signature[32..].copy_from_slice(s.as_bytes());
	signature
}

/// A signature that holds only under the cofactored equation: its `R` carries an order-8
/// component, which `8R` cancels and the cofactorless check does not.
fn cofactored_only(payload: &[u8]) -> [u8; 64] {
	let nonce = Scalar::from_bytes_mod_order([3; 32]);
	let r = (ED25519_BASEPOINT_POINT * nonce + EIGHT_TORSION[1]).compress().to_bytes();
	let s = nonce + challenge(&r, &public_key(&SECRET), payload) * secret_scalar();
	signature_of(&r, s)
}

/// A signature the cofactorless equation accepts under the identity key: with `A` the
/// identity, `S·B = R + k·A` holds for `R = S·B`. Only the small-order key is wrong.
fn valid_under_identity_key() -> [u8; 64] {
	let s = Scalar::from_bytes_mod_order([5; 32]);
	signature_of(&(ED25519_BASEPOINT_POINT * s).compress().to_bytes(), s)
}

/// A signature by `SECRET` the equation accepts with `R` the identity: `S = k·a` gives
/// `S·B = k·A = R + k·A`. Only the small-order `R` is wrong.
fn valid_with_identity_r(payload: &[u8]) -> [u8; 64] {
	let r = identity_point();
	signature_of(&r, challenge(&r, &public_key(&SECRET), payload) * secret_scalar())
}

/// The Ed25519 group order `L`, little-endian.
const GROUP_ORDER: [u8; 32] = [
	0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
	0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

/// `y = p + 3`: a point of large order under a non-canonical encoding, so it breaks only the
/// encoding rule. No signature can verify under a key or `R` nobody knows the discrete log of,
/// so these cases also fail the equation: readers check encodings first.
fn non_canonical_point() -> [u8; 32] {
	let mut encoding = [0xff; 32];
	encoding[0] = 0xf0;
	encoding[31] = 0x7f;
	encoding
}

fn identity_point() -> [u8; 32] {
	let mut encoding = [0; 32];
	encoding[0] = 1;
	encoding
}

struct DocumentCase {
	name: &'static str,
	bytes: Vec<u8>,
	key: Option<String>,
	expect: &'static str,
}

fn document_cases() -> Vec<DocumentCase> {
	let plain =
		|name, value: Value, expect| DocumentCase { name, bytes: bytes(&value), key: None, expect };
	let raw = |name, text: &str, expect| DocumentCase {
		name,
		bytes: text.as_bytes().to_vec(),
		key: None,
		expect,
	};
	let keyed =
		|name, bytes, key: String, expect| DocumentCase { name, bytes, key: Some(key), expect };
	let (payload, signature) = signature_parts();
	let tampered = |edit: &dyn Fn(&mut [u8; 64])| {
		let mut signature = signature;
		edit(&mut signature);
		envelope(1, &payload, &signature)
	};
	let compact = serde_json::to_string(&base()).unwrap();
	vec![
		plain("valid-full", base(), "ok"),
		plain(
			"valid-minimal",
			json!({
				"$v": 1, "sequence": 0, "generation": "g", "id": "local", "name": "Local",
				"network": "local", "tld": "local", "services": [],
				"chains": [{ "role": "Relay", "genesisHash": RELAY, "rpc": ["ws://127.0.0.1:9944"] }],
			}),
			"ok",
		),
		plain(
			"valid-private-range",
			with(|value| {
				value["chains"][0]["rpc"] = json!(["ws://10.0.0.5:9944", "ws://[fd00::5]:9944"]);
				value["services"][0]["urls"] = json!(["http://192.168.1.10/dub/"]);
			}),
			"ok",
		),
		plain(
			"valid-unknown-fields",
			with(|value| {
				value["futureField"] = json!({ "anything": [1, 2, 3] });
				value["chains"][0]["futureField"] = json!(true);
			}),
			"ok",
		),
		plain(
			"valid-unknown-service-kind",
			with(|value| {
				value["services"]
					.as_array_mut()
					.unwrap()
					.push(json!({ "kind": "future", "urls": "not a list" }))
			}),
			"ok",
		),
		plain("valid-fork", fork(), "ok"),
		keyed(
			"valid-fork-signed-with-own-key",
			sign(&bytes(&fork()), &OTHER_SECRET).unwrap(),
			hex(&public_key(&OTHER_SECRET)),
			"ok",
		),
		keyed("valid-signed", signed(&base()), key(), "ok"),
		DocumentCase {
			name: "valid-envelope-read-unsigned",
			bytes: signed(&base()),
			key: None,
			expect: "ok",
		},
		raw("not-json-trailing", &format!("{compact} x"), "not-json"),
		DocumentCase {
			name: "not-json-invalid-utf8",
			bytes: compact
				.replace("Previewnet", "Preview\0net")
				.into_bytes()
				.into_iter()
				.map(|byte| if byte == 0 { 0xff } else { byte })
				.collect(),
			key: None,
			expect: "not-json",
		},
		raw("not-json-lone-surrogate", &compact.replace("Previewnet", "\\ud800"), "not-json"),
		raw(
			"duplicate-member-top-level",
			&compact.replacen("{", r#"{"id":"other","#, 1),
			"duplicate-member",
		),
		raw(
			"duplicate-member-in-contracts",
			&compact.replace(
				r#""contracts":{"#,
				&format!(r#""contracts":{{"accountDataStore":"{}","#, hex(&[0x33; 20])),
			),
			"duplicate-member",
		),
		raw(
			"duplicate-member-in-ignored-field",
			&compact.replacen("{", r#"{"futureField":{"a":1,"a":2},"#, 1),
			"duplicate-member",
		),
		plain("unsupported-version", with(|value| value["$v"] = json!(2)), "unsupported-version"),
		plain(
			"malformed-sequence-type",
			with(|value| value["sequence"] = json!("42")),
			"malformed",
		),
		plain(
			"malformed-missing-chains",
			with(|value| {
				value.as_object_mut().unwrap().remove("chains");
			}),
			"malformed",
		),
		plain(
			"sequence-out-of-range",
			with(|value| value["sequence"] = json!(1u64 << 32)),
			"malformed",
		),
		plain("sequence-at-maximum", with(|value| value["sequence"] = json!(u32::MAX)), "ok"),
		raw(
			"sequence-with-fraction",
			&compact.replace(r#""sequence":42"#, r#""sequence":42.0"#),
			"malformed",
		),
		plain(
			"asset-id-out-of-range",
			with(|value| value["assets"][0]["id"] = json!(1u64 << 32)),
			"malformed",
		),
		raw(
			"noncharacter-in-name",
			&compact.replace("Previewnet", "Preview\u{fdd0}net"),
			"not-json",
		),
		plain(
			"rpc-ipv4-shorthand",
			with(|value| value["chains"][0]["rpc"] = json!(["ws://10.1:9944"])),
			"invalid-url",
		),
		plain(
			"rpc-ipv4-shorthand-over-tls",
			with(|value| value["chains"][0]["rpc"] = json!(["wss://10.1/relay"])),
			"invalid-url",
		),
		plain("tld-with-leading-dot", with(|value| value["tld"] = json!(".dot")), "invalid-tld"),
		plain("tld-uppercase", with(|value| value["tld"] = json!("DOT")), "invalid-tld"),
		plain("empty-generation", with(|value| value["generation"] = json!("")), "empty"),
		plain("empty-rpc", with(|value| value["chains"][1]["rpc"] = json!([])), "empty"),
		plain(
			"duplicate-chain-role",
			with(|value| value["chains"][2]["role"] = json!("AssetHub")),
			"duplicate-chain-role",
		),
		plain(
			"duplicate-service-kind",
			with(|value| value["services"][1]["kind"] = json!("dub")),
			"duplicate-service-kind",
		),
		plain(
			"duplicate-asset-role",
			with(|value| {
				value["assets"].as_array_mut().unwrap().push(
					json!({ "role": "payment", "chain": "People", "pallet": "Assets", "id": 1 }),
				)
			}),
			"duplicate-asset-role",
		),
		plain(
			"no-relay",
			with(|value| {
				value["chains"].as_array_mut().unwrap().remove(0);
			}),
			"relay-count",
		),
		plain("para-id-on-relay", with(|value| value["chains"][0]["paraId"] = json!(0)), "para-id"),
		plain(
			"para-id-missing",
			with(|value| {
				value["chains"][1].as_object_mut().unwrap().remove("paraId");
			}),
			"para-id",
		),
		plain(
			"para-id-repeated",
			with(|value| value["chains"][2]["paraId"] = json!(1000)),
			"para-id",
		),
		plain(
			"para-id-null-on-relay",
			with(|value| value["chains"][0]["paraId"] = json!(null)),
			"malformed",
		),
		plain(
			"asset-on-unknown-chain",
			with(|value| value["assets"][0]["chain"] = json!("Bulletin")),
			"unknown-chain-role",
		),
		plain(
			"coinage-on-unknown-chain",
			with(|value| value["coinage"]["chain"] = json!("Bulletin")),
			"unknown-chain-role",
		),
		plain(
			"time-as-string",
			with(|value| value["validUntil"] = json!("2026-10-14T00:00:00Z")),
			"malformed",
		),
		plain(
			"time-out-of-range",
			with(|value| value["expiresAt"] = json!(1u64 << 32)),
			"malformed",
		),
		plain(
			"genesis-hash-uppercase",
			with(|value| {
				value["chains"][0]["genesisHash"] = json!(RELAY.to_uppercase().replace("0X", "0x"))
			}),
			"invalid-hex",
		),
		plain(
			"sha256-short",
			with(|value| value["chains"][1]["lightSpec"]["sha256"] = json!(hex(&[0x11; 31]))),
			"invalid-hex",
		),
		plain(
			"contract-address-long",
			with(|value| value["contracts"]["accountDataStore"] = json!(hex(&[0x22; 32]))),
			"invalid-hex",
		),
		plain(
			"rpc-plain-ws-on-hostname",
			with(|value| {
				value["chains"][0]["rpc"] = json!(["ws://previewnet.substrate.dev/relay"])
			}),
			"invalid-url",
		),
		plain(
			"rpc-plain-ws-on-localhost-name",
			with(|value| value["chains"][0]["rpc"] = json!(["ws://localhost:9944"])),
			"invalid-url",
		),
		plain(
			"rpc-without-authority",
			with(|value| value["chains"][0]["rpc"] = json!(["wss:previewnet.substrate.dev/relay"])),
			"invalid-url",
		),
		plain(
			"stun-scheme-outside-stun",
			with(|value| {
				value["services"][0]["urls"] = json!(["stun:previewnet.substrate.dev:3478"])
			}),
			"invalid-url",
		),
		plain(
			"service-url-not-base",
			with(|value| {
				value["services"][0]["urls"] = json!(["https://previewnet.substrate.dev/dub"])
			}),
			"service-url-not-base",
		),
		plain(
			"service-url-with-query",
			with(|value| {
				value["services"][0]["urls"] = json!(["https://previewnet.substrate.dev/dub/?v=/"])
			}),
			"service-url-not-base",
		),
		plain(
			"eth-rpc-without-chain-id",
			with(|value| {
				value["services"][2].as_object_mut().unwrap().remove("chainId");
			}),
			"chain-id",
		),
		plain(
			"chain-id-on-dub",
			with(|value| value["services"][0]["chainId"] = json!(1)),
			"chain-id",
		),
		plain(
			"chain-id-above-u32",
			with(|value| value["services"][2]["chainId"] = json!(11_297_108_109u64)),
			"ok",
		),
		plain(
			"chain-id-above-2-53",
			with(|value| value["services"][2]["chainId"] = json!(1u64 << 53)),
			"chain-id",
		),
		plain(
			"url-with-credentials",
			with(|value| {
				value["services"][0]["urls"] =
					json!(["https://previewnet.substrate.dev@evil.example/"])
			}),
			"invalid-url",
		),
		plain(
			"fork-without-forked-at",
			with(|value| value["fork"] = json!({ "of": { "url": URL }, "at": TWO_HOURS_AGO })),
			"forked-at",
		),
		plain(
			"forked-at-without-fork",
			with(|value| {
				value["chains"][0]["forkedAt"] = json!({ "number": 1, "hash": hex(&[1; 32]) })
			}),
			"forked-at",
		),
		raw(
			"envelope-invalid-base64",
			&format!(r#"{{"$v":1,"payload":"!!!","signature":"{}"}}"#, hex(&signature)),
			"invalid-base64",
		),
		keyed(
			"envelope-version-mismatch",
			envelope_signed(1, &bytes(&with(|value| value["$v"] = json!(2)))),
			key(),
			"envelope-version-mismatch",
		),
		keyed(
			"envelope-version-unsupported",
			envelope_signed(2, &bytes(&with(|value| value["$v"] = json!(2)))),
			key(),
			"unsupported-version",
		),
		keyed("envelope-in-envelope", envelope_signed(1, &signed(&base())), key(), "malformed"),
		keyed(
			"fork-signed-with-source-key",
			envelope_signed(1, &bytes(&fork())),
			key(),
			"fork-signed-with-source-key",
		),
		keyed("keyed-pointer-plain-document", bytes(&base()), key(), "unsigned"),
		keyed(
			"key-non-canonical",
			signed(&base()),
			hex(&non_canonical_point()),
			"non-canonical-key",
		),
		keyed(
			"key-small-order",
			envelope(1, &payload, &valid_under_identity_key()),
			hex(&identity_point()),
			"small-order-key",
		),
		keyed(
			"signature-r-non-canonical",
			tampered(&|signature| signature[..32].copy_from_slice(&non_canonical_point())),
			key(),
			"non-canonical-r",
		),
		keyed(
			"signature-r-small-order",
			envelope(1, &payload, &valid_with_identity_r(&payload)),
			key(),
			"small-order-r",
		),
		keyed(
			"signature-s-not-reduced",
			tampered(&|signature| {
				let mut carry = 0u16;
				for index in 0..32 {
					let sum =
						u16::from(signature[32 + index]) + u16::from(GROUP_ORDER[index]) + carry;
					signature[32 + index] = sum as u8;
					carry = sum >> 8;
				}
			}),
			key(),
			"non-canonical-s",
		),
		keyed(
			"signature-other-key",
			signed(&base()),
			hex(&public_key(&OTHER_SECRET)),
			"bad-signature",
		),
		keyed(
			"signature-without-prefix",
			{
				use ed25519_dalek::{Signer, SigningKey};
				let signature = SigningKey::from_bytes(&SECRET).sign(&payload).to_bytes();
				envelope(1, &payload, &signature)
			},
			key(),
			"bad-signature",
		),
		keyed(
			"signature-cofactored-only",
			envelope(1, &payload, &cofactored_only(&payload)),
			key(),
			"bad-signature",
		),
	]
}

struct ReaderCase {
	name: &'static str,
	signed: bool,
	stored: Option<Vec<u8>>,
	fetched: Option<Vec<u8>>,
	expect: Value,
}

fn reader_cases() -> Vec<ReaderCase> {
	let at = |sequence: u64, edit: &dyn Fn(&mut Value)| {
		let mut value = base();
		value["sequence"] = json!(sequence);
		edit(&mut value);
		signed(&value)
	};
	let plain_at = |sequence: u64| bytes(&with(|value| value["sequence"] = json!(sequence)));
	let same = |_: &mut Value| {};
	let outcome = |status: &str, keeps: &str| json!({ "status": status, "keeps": keeps, "newGeneration": false, "stale": false, "expired": false });
	let case = |name, stored: Option<Vec<u8>>, fetched: Option<Vec<u8>>, expect| ReaderCase {
		name,
		signed: true,
		stored,
		fetched,
		expect,
	};
	vec![
		case("first-fetch", None, Some(at(42, &same)), outcome("updated", "fetched")),
		case(
			"higher-sequence",
			Some(at(42, &same)),
			Some(at(43, &|value| value["name"] = json!("Renamed"))),
			outcome("updated", "fetched"),
		),
		case(
			"new-generation",
			Some(at(42, &same)),
			Some(at(43, &|value| value["generation"] = json!("2026-10-15T00:00:00Z"))),
			json!({ "status": "updated", "keeps": "fetched", "newGeneration": true, "stale": false, "expired": false }),
		),
		case(
			"same-content",
			Some(at(42, &same)),
			Some(at(42, &same)),
			outcome("unchanged", "stored"),
		),
		case(
			"same-sequence-different-content",
			Some(at(42, &same)),
			Some(at(42, &|value| value["name"] = json!("Renamed"))),
			outcome("conflict", "stored"),
		),
		case(
			"lower-sequence-signed",
			Some(at(42, &same)),
			Some(at(41, &same)),
			outcome("rollback", "stored"),
		),
		ReaderCase {
			name: "lower-sequence-unsigned",
			signed: false,
			stored: Some(plain_at(42)),
			fetched: Some(plain_at(41)),
			expect: outcome("updated", "fetched"),
		},
		case(
			"unreachable-with-stored",
			Some(at(42, &same)),
			None,
			outcome("unreachable", "stored"),
		),
		case("unreachable-without-stored", None, None, json!({ "error": "unreachable" })),
		case(
			"refused-with-stored",
			Some(at(42, &same)),
			Some(bytes(&base())),
			json!({ "status": "refused", "refused": "unsigned", "keeps": "stored", "newGeneration": false, "stale": false, "expired": false }),
		),
		case("refused-without-stored", None, Some(bytes(&base())), json!({ "error": "unsigned" })),
		case(
			"stale",
			None,
			Some(at(42, &|value| value["validUntil"] = json!(YESTERDAY))),
			json!({ "status": "updated", "keeps": "fetched", "newGeneration": false, "stale": true, "expired": false }),
		),
		case(
			"expired",
			None,
			Some(at(42, &|value| value["expiresAt"] = json!(ONE_HOUR_AGO))),
			json!({ "status": "updated", "keeps": "fetched", "newGeneration": false, "stale": false, "expired": true }),
		),
	]
}

fn link_cases() -> Vec<(&'static str, String, Value)> {
	let encoded = "https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json";
	vec![
		(
			"rfc-link",
			format!("polkadotapp://stack?key={}&url={encoded}", key()),
			json!({ "url": URL, "key": key() }),
		),
		(
			"https-link-extra-parameters",
			format!("https://app.example/open?ref=qr&url={encoded}"),
			json!({ "url": URL }),
		),
		(
			"no-url",
			format!("polkadotapp://stack?key={}", key()),
			json!({ "error": "invalid-link" }),
		),
		(
			"repeated-url",
			format!("polkadotapp://stack?url={encoded}&url={encoded}"),
			json!({ "error": "invalid-link" }),
		),
		(
			"plain-http-hostname",
			"polkadotapp://stack?url=http%3A%2F%2Fexample.com%2Fs.json".into(),
			json!({ "error": "invalid-url" }),
		),
		(
			"key-uppercase",
			format!(
				"polkadotapp://stack?key={}&url={encoded}",
				key().to_uppercase().replace("0X", "0x")
			),
			json!({ "error": "invalid-hex" }),
		),
	]
}

fn generate() -> BTreeMap<PathBuf, Vec<u8>> {
	let mut files = BTreeMap::new();
	let mut file = |path: String, contents: Vec<u8>| {
		files.insert(PathBuf::from(&path), contents);
		path
	};
	let documents: Vec<Value> = document_cases()
		.into_iter()
		.map(|case| {
			let path = file(format!("documents/{}.json", case.name), case.bytes);
			json!({ "name": case.name, "file": path, "pointer": { "url": URL, "key": case.key }, "expect": case.expect })
		})
		.collect();
	let reader: Vec<Value> = reader_cases()
		.into_iter()
		.map(|case| {
			let stored = case.stored.map(|bytes| file(format!("reader/{}.stored.json", case.name), bytes));
			let fetched = case.fetched.map(|bytes| file(format!("reader/{}.fetched.json", case.name), bytes));
			let key = case.signed.then(key);
			json!({ "name": case.name, "pointer": { "url": URL, "key": key }, "stored": stored, "fetched": fetched, "now": NOW, "expect": case.expect })
		})
		.collect();
	let links: Vec<Value> = link_cases()
		.into_iter()
		.map(|(name, link, expect)| json!({ "name": name, "link": link, "expect": expect }))
		.collect();
	let cases = json!({ "documents": documents, "reader": reader, "links": links });
	let mut cases = serde_json::to_vec_pretty(&cases).unwrap();
	cases.push(b'\n');
	files.insert(PathBuf::from("cases.json"), cases);
	files
}

fn vectors_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors")
}

fn committed() -> BTreeMap<PathBuf, Vec<u8>> {
	let root = vectors_dir();
	let mut files = BTreeMap::new();
	let mut pending = vec![root.clone()];
	while let Some(dir) = pending.pop() {
		for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
			let path = entry.path();
			if path.is_dir() {
				pending.push(path);
			} else {
				files
					.insert(path.strip_prefix(&root).unwrap().to_owned(), fs::read(&path).unwrap());
			}
		}
	}
	files
}

#[test]
fn committed_vectors_match_the_generator() {
	let generated = generate();
	if std::env::var_os("TRINITY_STACK_WRITE_VECTORS").is_some() {
		let root = vectors_dir();
		for stale in committed().keys().filter(|path| !generated.contains_key(*path)) {
			fs::remove_file(root.join(stale)).unwrap();
		}
		for (path, contents) in &generated {
			fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
			fs::write(root.join(path), contents).unwrap();
		}
	}
	assert!(
		committed() == generated,
		"vectors/ differs from the generator; rerun with TRINITY_STACK_WRITE_VECTORS=1"
	);
}

fn pointer_of(case: &Value) -> Pointer {
	Pointer::new(case["pointer"]["url"].as_str().unwrap(), case["pointer"]["key"].as_str()).unwrap()
}

#[test]
fn sign_refuses_a_fork_with_its_sources_key() {
	let refused = sign(&bytes(&fork()), &SECRET).map_err(|error| error.code());
	assert_eq!(refused, Err("fork-signed-with-source-key"));
}

/// The small-order cases must break only the small-order rule, so a reader that checks the
/// equation first still gets the expected code. dalek's non-strict `verify` runs the same
/// cofactorless equation without refusing small-order points.
#[test]
fn small_order_cases_pass_the_equation_and_the_non_canonical_point_is_not_small() {
	use curve25519_dalek::edwards::CompressedEdwardsY;
	use ed25519_dalek::{Signature, Verifier, VerifyingKey};
	let (payload, _) = signature_parts();
	let message = [b"trinity-stack/descriptor/v1".as_slice(), &payload].concat();
	let identity = VerifyingKey::from_bytes(&identity_point()).unwrap();
	assert!(
		identity
			.verify(&message, &Signature::from_bytes(&valid_under_identity_key()))
			.is_ok()
	);
	let ours = VerifyingKey::from_bytes(&public_key(&SECRET)).unwrap();
	let identity_r = Signature::from_bytes(&valid_with_identity_r(&payload));
	assert!(ours.verify(&message, &identity_r).is_ok());
	let point = CompressedEdwardsY(non_canonical_point()).decompress().unwrap();
	assert!(!point.is_small_order());
}

/// Runs the committed files, as any other reader would.
#[test]
fn every_vector_gets_its_verdict() {
	let cases: Value =
		serde_json::from_slice(&fs::read(vectors_dir().join("cases.json")).unwrap()).unwrap();
	let file = |path: &Value| path.as_str().map(|path| fs::read(vectors_dir().join(path)).unwrap());

	for case in cases["documents"].as_array().unwrap() {
		let verdict = open(&file(&case["file"]).unwrap(), &pointer_of(case));
		let verdict = verdict.map_or_else(|error| error.code(), |_| "ok");
		assert_eq!(verdict, case["expect"], "{}", case["name"]);
	}

	for case in cases["reader"].as_array().unwrap() {
		let stored = file(&case["stored"]);
		let fetched = file(&case["fetched"]);
		let outcome = accept(
			&pointer_of(case),
			stored.as_deref(),
			fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document),
			case["now"].as_i64().unwrap(),
		);
		let outcome = match outcome {
			Err(error) => json!({ "error": error.code() }),
			Ok(accepted) => {
				let keeps =
					if Some(&accepted.document) == stored.as_ref() { "stored" } else { "fetched" };
				let mut outcome = json!({
					"status": accepted.status.code(),
					"keeps": keeps,
					"newGeneration": accepted.new_generation,
					"stale": accepted.stale,
					"expired": accepted.expired,
				});
				if let trinity_stack::Status::Refused(error) = &accepted.status {
					outcome["refused"] = json!(error.code());
				}
				outcome
			},
		};
		assert_eq!(outcome, case["expect"], "{}", case["name"]);
	}

	for case in cases["links"].as_array().unwrap() {
		let outcome = match Pointer::from_link(case["link"].as_str().unwrap()) {
			Err(error) => json!({ "error": error.code() }),
			Ok(pointer) => {
				let mut outcome = json!({ "url": pointer.url().as_str() });
				if let Some(key) = pointer.key() {
					outcome["key"] = json!(hex(key));
				}
				outcome
			},
		};
		assert_eq!(outcome, case["expect"], "{}", case["name"]);
	}
}
