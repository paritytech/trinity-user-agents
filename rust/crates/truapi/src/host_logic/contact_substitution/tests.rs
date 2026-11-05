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

	let out = substitute(&call, &[(HANDLE, Some(ACCOUNT)), (OTHER_HANDLE, Some(OTHER_ACCOUNT))])
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
	let error = substitute(&call, &[(HANDLE, Some(ACCOUNT)), (OTHER_HANDLE, Some(OTHER_ACCOUNT))])
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
