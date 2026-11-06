use super::*;

#[test]
fn plain_http_only_on_private_literal_addresses() {
	for allowed in [
		"https://a.example/",
		"wss://a.example",
		"http://10.0.0.5/",
		"ws://[::1]:9944",
		"http://192.168.1.2",
		"wss://8.8.8.8/",
		"https://A.Example/",
		"https://xn--fa-hia.example/caf%C3%A9",
	] {
		assert!(url(allowed, Schemes::Web).is_ok(), "{allowed}");
	}
	for refused in [
		"http://localhost/",
		"http://8.8.8.8/",
		"ftp://a.example/",
		"stun:a.example:3478",
		"http://[2001:db8::1]/",
		"ws://10.1:9944",
		"ws://0x7f.1/",
		"http://user@10.1/",
		"https://previewnet.substrate.dev@evil.example/",
		"wss://user:secret@a.example/",
		"wss://10.1/",
		"https://0x08080808/",
	] {
		assert!(url(refused, Schemes::Web).is_err(), "{refused}");
	}
	assert!(url("stun:stun.example:3478", Schemes::Stun).is_ok());
	assert!(url("https://a.example/", Schemes::Stun).is_err());
}

#[test]
fn a_url_is_read_as_written() {
	for refused in [
		" https://a.example/",
		"https://a.example/\n",
		"https://a.exa\tmple/",
		"wss:a.example",
		"https://a.example\\@evil.example/",
		"https://faß.example/",
		"https://a.example/café",
	] {
		assert!(url(refused, Schemes::Web).is_err(), "{refused:?}");
	}
	assert!(url("stun:stun.example:3478 ", Schemes::Stun).is_err());
}

#[test]
fn hex_is_lowercase_and_sized() {
	assert_eq!(hex_bytes::<2>("f", "0xabcd"), Ok([0xab, 0xcd]));
	for invalid in ["abcd", "0xABCD", "0xabc", "0xabcdef"] {
		assert_eq!(hex_bytes::<2>("f", invalid), Err(Error::InvalidHex { field: "f", bytes: 2 }));
	}
}
