//! How protocol values with no UniFFI counterpart cross FFI surfaces.

use crate::{Bytes32, Bytes64};

uniffi::custom_type!(Bytes32, Vec<u8>, {
	remote,
	lower: |bytes| bytes.to_vec(),
	try_lift: |bytes| Ok(bytes.as_slice().try_into()?),
});

uniffi::custom_type!(Bytes64, Vec<u8>, {
	remote,
	lower: |bytes| bytes.to_vec(),
	try_lift: |bytes| Ok(bytes.as_slice().try_into()?),
});

// Swift and Kotlin have no 128-bit integer, so amounts cross FFI surfaces as
// decimal strings.
uniffi::custom_type!(u128, String, {
	remote,
	lower: |amount| amount.to_string(),
	try_lift: |amount| Ok(amount.parse()?),
});

// UniFFI has no arrays, so a material argument's vector crosses FFI surfaces as
// a list. The aliases exist because `uniffi::custom_type!` takes an ident.
macro_rules! u32_array_as_list {
	($alias:ident, $len:literal) => {
		type $alias = [u32; $len];
		uniffi::custom_type!($alias, Vec<u32>, {
			remote,
			lower: |values| values.to_vec(),
			try_lift: |values| Ok(values.as_slice().try_into()?),
		});
	};
}

u32_array_as_list!(U32x2, 2);
u32_array_as_list!(U32x3, 3);
u32_array_as_list!(U32x4, 4);
