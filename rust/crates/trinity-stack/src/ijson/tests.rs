use super::*;

#[test]
fn refuses_a_duplicate_member_at_any_depth() {
	assert_eq!(parse(br#"{"a":1,"a":2}"#), Err(Error::DuplicateMember("a".into())));
	assert_eq!(parse(br#"{"x":[{"b":{"c":1,"c":1}}]}"#), Err(Error::DuplicateMember("c".into())));
}

#[test]
fn refuses_what_is_not_json() {
	assert_eq!(parse(b"{} trailing").map_err(|error| error.code()), Err("not-json"));
	assert_eq!(parse(b"\"\xff\"").map_err(|error| error.code()), Err("not-json"));
	assert_eq!(parse(br#""\ud800""#).map_err(|error| error.code()), Err("not-json"));
	for noncharacter in [r#"{"a":"￿"}"#, "{\"\u{fdd0}\":1}", "[\"\u{10fffe}\"]"] {
		assert_eq!(
			parse(noncharacter.as_bytes()).map_err(|error| error.code()),
			Err("not-json"),
			"{noncharacter:?}"
		);
	}
	assert!(parse("[\"\u{fdcf}\u{fffd}\"]".as_bytes()).is_ok());
}
