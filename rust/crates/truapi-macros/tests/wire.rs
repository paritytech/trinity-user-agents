//! A protocol consumer can use the wire attributes independently of the worker runtime.

use truapi_macros::{wire, wire_trait};

#[wire_trait(id = 1)]
trait Sequence {
	#[wire(id = 0)]
	fn next_value(&self) -> u32;
}

impl Sequence for u32 {
	fn next_value(&self) -> u32 {
		self + 1
	}
}

#[test]
fn standalone_wire_traits_can_use_their_own_method_signatures() {
	assert_eq!(41.next_value(), 42);
}
