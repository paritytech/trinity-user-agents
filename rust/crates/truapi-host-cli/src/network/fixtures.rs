//! Stack descriptors for tests, edited from the bundled paseo-next-v2 copy.

use trinity_stack::{Descriptor, Document};

const PASEO_NEXT_V2_BUNDLE: &[u8] = include_bytes!("../../stacks/paseo-next-v2.json");

/// The bundled paseo-next-v2 descriptor, edited, as plain JSON bytes.
pub fn edit_descriptor(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
	let Ok(Document::Envelope { payload, .. }) = Document::parse(PASEO_NEXT_V2_BUNDLE) else {
		panic!("bundled paseo-next-v2 stack is not an envelope");
	};
	let mut descriptor: serde_json::Value =
		serde_json::from_slice(&payload).expect("invalid bundled payload");
	edit(&mut descriptor);
	serde_json::to_vec(&descriptor).expect("JSON value does not serialise")
}

/// The bundled paseo-next-v2 descriptor, edited.
pub fn edit_stack(edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	let plain = edit_descriptor(edit);
	Document::parse(&plain).expect("invalid test descriptor").descriptor().clone()
}

/// The bundled paseo-next-v2 descriptor with `role`'s chain edited.
pub fn edit_chain(role: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	edit_stack(|descriptor| {
		let chains = descriptor["chains"].as_array_mut().expect("chains is not an array");
		edit(chains.iter_mut().find(|chain| chain["role"] == role).expect("role missing"));
	})
}
