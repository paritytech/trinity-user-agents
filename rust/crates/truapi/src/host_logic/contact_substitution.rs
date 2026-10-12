//! Replacing contact handles with the accounts they name, inside call data.
//!
//! A product builds its own call and can put a handle wherever an account goes,
//! so the core has no business parsing the call to find it. Instead the product
//! declares which handles the call names, and this replaces exactly those
//! 32-byte runs. Anything the product did not declare is left alone, so a call
//! that happens to contain other bytes is untouched.
//!
//! Declaring rather than passing an offset is what makes it checkable: an
//! offset is a number the product computes about its own SCALE encoding and
//! gets wrong silently, while a declared handle is either in the call or it is
//! not, and this says which. A false match would need a keyed 32-byte hash to
//! occur by chance.

/// Why a call's handles could not be substituted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstitutionError {
	/// A declared handle names nobody the host has a contact for. A stale
	/// handle from a removed contact looks the same as a forged one, which is
	/// the only revocation this API has.
	UnknownContact,
	/// A declared handle does not appear in the call data. The product built a
	/// call that does not name the person it says it does, and signing it would
	/// pay somebody else.
	HandleNotInCall,
}

/// Replace every occurrence of each declared handle with its account.
///
/// The pairs are `(handle, account)`, resolved by the caller. Substitution is
/// all-or-nothing: an error leaves no partially rewritten call behind for a
/// caller to sign by accident.
pub fn substitute(
	call_data: &[u8],
	resolved: &[([u8; 32], Option<[u8; 32]>)],
) -> Result<Vec<u8>, SubstitutionError> {
	let mut out = call_data.to_vec();
	let mut done: Vec<[u8; 32]> = Vec::new();
	for (handle, account) in resolved {
		let account = account.ok_or(SubstitutionError::UnknownContact)?;
		// Declaring is naming which contacts the call mentions, so naming one
		// twice is the same statement made twice. A batch paying one person in
		// two calls reads its handles per call, and the second pass would find
		// nothing left to replace.
		if done.contains(handle) {
			continue;
		}
		done.push(*handle);
		let mut replaced = false;
		let mut at = 0;
		while let Some(found) = find(&out[at..], handle) {
			let start = at + found;
			out[start..start + 32].copy_from_slice(&account);
			at = start + 32;
			replaced = true;
		}
		if !replaced {
			return Err(SubstitutionError::HandleNotInCall);
		}
	}
	Ok(out)
}

/// First index of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8; 32]) -> Option<usize> {
	haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(test)]
mod tests {
	use super::*;

	const HANDLE: [u8; 32] = [0xAA; 32];
	const OTHER_HANDLE: [u8; 32] = [0xBB; 32];
	const ACCOUNT: [u8; 32] = [0x11; 32];
	const OTHER_ACCOUNT: [u8; 32] = [0x22; 32];

	/// A transfer-shaped call: a two-byte call index, the recipient, an amount.
	fn call_naming(handle: [u8; 32]) -> Vec<u8> {
		let mut call = vec![0x04, 0x00];
		call.extend_from_slice(&handle);
		call.extend_from_slice(&[0x07; 8]);
		call
	}

	#[test]
	fn the_account_takes_the_handles_place_and_nothing_else_moves() {
		let call = call_naming(HANDLE);
		let out = substitute(&call, &[(HANDLE, Some(ACCOUNT))]).expect("the handle is in the call");

		assert_eq!(&out[..2], &[0x04, 0x00], "the call index is untouched");
		assert_eq!(&out[2..34], &ACCOUNT, "the recipient is the account");
		assert_eq!(&out[34..], &[0x07; 8], "the amount is untouched");
		assert_eq!(out.len(), call.len(), "an account is as wide as a handle");
	}

	#[test]
	fn one_person_paid_twice_is_substituted_twice() {
		// A batch naming the same contact in two calls. Replacing only the
		// first would sign a transaction that pays a handle to nobody.
		let mut call = call_naming(HANDLE);
		call.extend_from_slice(&call_naming(HANDLE));

		let out = substitute(&call, &[(HANDLE, Some(ACCOUNT))]).expect("both are found");

		assert_eq!(out.windows(32).filter(|w| *w == ACCOUNT).count(), 2);
		assert!(!out.windows(32).any(|w| w == HANDLE));
	}

	#[test]
	fn several_contacts_in_one_call_each_get_their_own_account() {
		let mut call = call_naming(HANDLE);
		call.extend_from_slice(&call_naming(OTHER_HANDLE));

		let out =
			substitute(&call, &[(HANDLE, Some(ACCOUNT)), (OTHER_HANDLE, Some(OTHER_ACCOUNT))])
				.expect("both are found");

		assert!(out.windows(32).any(|w| w == ACCOUNT));
		assert!(out.windows(32).any(|w| w == OTHER_ACCOUNT));
	}

	#[test]
	fn a_handle_the_host_cannot_resolve_refuses_the_whole_call() {
		assert_eq!(
			substitute(&call_naming(HANDLE), &[(HANDLE, None)]),
			Err(SubstitutionError::UnknownContact)
		);
	}

	#[test]
	fn a_declared_handle_the_call_does_not_name_refuses_it() {
		// The product said it was paying someone the call does not mention.
		// Signing it anyway would send the money somewhere else entirely.
		assert_eq!(
			substitute(&call_naming(HANDLE), &[(OTHER_HANDLE, Some(OTHER_ACCOUNT))]),
			Err(SubstitutionError::HandleNotInCall)
		);
	}

	#[test]
	fn a_failure_leaves_no_half_rewritten_call() {
		let call = call_naming(HANDLE);
		let error =
			substitute(&call, &[(HANDLE, Some(ACCOUNT)), (OTHER_HANDLE, Some(OTHER_ACCOUNT))])
				.expect_err("the second handle is not in the call");

		assert_eq!(error, SubstitutionError::HandleNotInCall);
	}

	/// A batch paying one person twice reads its handles per call, so it
	/// declares the same one twice. That names a contact the call does name.
	#[test]
	fn one_contact_declared_twice_is_still_substituted() {
		let mut call = call_naming(HANDLE);
		call.extend_from_slice(&call_naming(HANDLE));

		let out = substitute(&call, &[(HANDLE, Some(ACCOUNT)), (HANDLE, Some(ACCOUNT))])
			.expect("naming one contact twice names a contact the call names");

		assert_eq!(out.windows(32).filter(|w| *w == ACCOUNT).count(), 2);
		assert!(!out.windows(32).any(|w| w == HANDLE));
	}

	/// Deduplicating declarations must not excuse one that is genuinely absent.
	#[test]
	fn a_repeated_declaration_still_needs_to_be_in_the_call() {
		assert_eq!(
			substitute(
				&call_naming(HANDLE),
				&[(OTHER_HANDLE, Some(OTHER_ACCOUNT)), (OTHER_HANDLE, Some(OTHER_ACCOUNT))]
			),
			Err(SubstitutionError::HandleNotInCall)
		);
	}

	#[test]
	fn a_call_naming_nobody_is_returned_as_it_stands() {
		let call = call_naming(HANDLE);
		assert_eq!(substitute(&call, &[]), Ok(call.clone()));
	}
}
