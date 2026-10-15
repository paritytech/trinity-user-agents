//! JavaScript error values exposed by the WASM host runtime.

#![cfg(all(target_arch = "wasm32", feature = "runtime"))]

use truapi::latest::GenericError;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn generic_errors_reject_with_plain_reason_strings() {
	let outcome: Result<(), JsValue> = (|| {
		Err::<(), _>(GenericError { reason: "storage unavailable".to_string() })?;
		Ok(())
	})();

	assert_eq!(outcome, Err(JsValue::from_str("storage unavailable")));
}
