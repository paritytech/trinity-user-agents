mod pairing;
mod signing;

use super::{AccountHolder, HostAccounts};

/// The revision `accounts` keeps its grants under.
///
/// Releases the lifecycle guard before returning: read inline inside a call
/// that writes a grant, the guard would outlive the read and block the write.
fn grant_revision<H: AccountHolder>(accounts: &HostAccounts<H>) -> u64 {
	accounts.grants.lifecycle().revision()
}
