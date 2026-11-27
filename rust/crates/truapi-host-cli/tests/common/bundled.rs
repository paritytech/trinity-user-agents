use trinity_stack::{Descriptor, Document};

/// The bundled paseo-next-v2 descriptor.
pub fn paseo_next_v2() -> Descriptor {
	Document::parse(include_bytes!("../../../../../stacks/paseo-next-v2.json"))
		.expect("invalid bundled paseo-next-v2 stack")
		.descriptor()
		.clone()
}
