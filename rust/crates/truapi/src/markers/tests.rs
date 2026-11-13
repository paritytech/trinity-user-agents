use super::*;

/// The table and the lookup are two outputs of one list, so this holds
/// unless the macro itself is wrong.
#[test]
fn the_table_and_the_lookup_agree() {
	for marker in MARKERS {
		assert_eq!(class_of(marker.target), Some(marker.class), "{} is unreachable", marker.target);
	}
}

#[test]
fn every_target_is_host_addressed() {
	for marker in MARKERS {
		assert!(
			marker.target.starts_with(HOST_MARKER_PREFIX),
			"{} is not addressed to a host",
			marker.target
		);
	}
}

#[test]
fn an_unlisted_target_has_no_class() {
	assert_eq!(class_of("truapi.not.registered"), None);
	assert_eq!(class_of(CORE_STARTED.target), Some(CORE_STARTED.class));
}

#[test]
fn the_targets_the_layer_builds_are_registered() {
	for level in ["warn", "error"] {
		let target = format!("truapi.tracing.{level}");
		assert!(class_of(&target).is_some(), "the layer raises {target}, which is not registered");
	}
}

#[test]
fn native_callbacks_stay_on_the_device() {
	for marker in MARKERS.iter().filter(|m| m.target.starts_with("truapi.native.")) {
		assert_eq!(marker.class, MarkerClass::Local, "{} would be exported", marker.target);
	}
}
