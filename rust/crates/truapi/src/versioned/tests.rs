use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ProbeV1 {
	a: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ProbeV2 {
	b: Vec<u8>,
}

truapi_macros::versioned_type! {
	enum MultiVersionProbe {
		V1 => ProbeV1,
		V2 => ProbeV2,
	}
}

// Multi-version envelopes assign positional SCALE codec indices (V1 -> 0,
// V2 -> 1) and 1-based version numbers. The dispatcher reads that first
// byte to refuse a request newer than its handler knows.
#[test]
fn multi_version_codec_indices_are_positional() {
	use super::Versioned;

	let v1 = MultiVersionProbe::V1(ProbeV1 { a: 7 });
	let v2 = MultiVersionProbe::V2(ProbeV2 { b: b"hello".to_vec() });

	assert_eq!(v1.encode()[0], 0, "V1 encodes codec index 0");
	assert_eq!(v2.encode()[0], 1, "V2 encodes codec index 1");
	assert_eq!(v1.version(), 1);
	assert_eq!(v2.version(), 2);
	assert_eq!(MultiVersionProbe::LATEST, 2);
}

#[test]
fn v1_discriminant_is_zero() {
	let v1 = super::permissions::HostDevicePermissionRequest::V1(
		crate::v01::HostDevicePermissionRequest::Camera,
	);
	assert_eq!(v1.encode()[0], 0, "V1 must encode discriminant 0");
}

#[test]
fn device_permission_discriminants_are_append_only() {
	// Every grant the user has ever given is persisted under this
	// discriminant. Inserting a variant rather than appending one silently
	// repoints all of them, on every device, with nothing to notice it.
	use crate::v01::HostDevicePermissionRequest::*;
	for (index, permission) in [
		Notifications,
		Camera,
		Microphone,
		Bluetooth,
		NFC,
		Location,
		Clipboard,
		OpenUrl,
		Biometrics,
	]
	.into_iter()
	.enumerate()
	{
		assert_eq!(
			permission.encode(),
			vec![index as u8],
			"{permission} must keep discriminant {index}"
		);
	}
}

#[test]
fn unit_response_roundtrip() {
	let original = super::system::HostNavigateToResponse::V1;
	let decoded =
		super::system::HostNavigateToResponse::decode(&mut &original.encode()[..]).expect("decode");
	assert_eq!(original, decoded);
}

#[test]
fn struct_variant_roundtrip() {
	let original = super::local_storage::HostLocalStorageWriteRequest::V1(
		crate::v01::HostLocalStorageWriteRequest {
			key: "greeting".into(),
			value: b"hello".to_vec(),
		},
	);
	let decoded =
		super::local_storage::HostLocalStorageWriteRequest::decode(&mut &original.encode()[..])
			.expect("decode");
	assert_eq!(original, decoded);
}
