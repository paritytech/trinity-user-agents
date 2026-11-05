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
