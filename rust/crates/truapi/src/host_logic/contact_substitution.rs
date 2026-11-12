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
mod tests;
