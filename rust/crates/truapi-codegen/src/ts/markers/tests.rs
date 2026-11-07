use super::*;

#[test]
fn the_file_holds_every_marker_in_table_order() {
	let code = markers_code();
	let entries: Vec<&str> = code
		.lines()
		.filter(|line| line.starts_with("  ["))
		.map(|line| line.trim())
		.collect();
	assert_eq!(entries.len(), MARKERS.len(), "the file lost or gained an entry");
	for (entry, marker) in entries.iter().zip(MARKERS) {
		assert_eq!(*entry, format!("{},", map_entry(marker)));
	}
}

#[test]
fn an_exported_marker_carries_its_severity() {
	let entry = map_entry(&truapi::markers::TRACING_ERROR);
	assert_eq!(entry, r#"["truapi.tracing.error", { kind: "export", severity: "error" }]"#);
}

#[test]
fn a_local_marker_carries_no_severity() {
	let entry = map_entry(&truapi::markers::NATIVE_PUSH_NOTIFICATION);
	assert_eq!(entry, r#"["truapi.native.callback.push_notification", { kind: "local" }]"#);
}
