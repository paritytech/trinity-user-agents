//! Contacts host for the CLI.
//!
//! The list is read from `TRUAPI_CONTACTS` as `name=0x<32-byte account>`
//! entries separated by `;`. With nothing configured the host serves a small
//! development list, so the generated example and the battery have someone to
//! pick without a chat extension behind them. An empty spec is an empty list,
//! which is how `NoContacts` gets exercised.
//!
//! The picker is this host's own approval surface: it names the contact it
//! would return and takes yes or no. A headless run auto-approves it, an
//! interactive one asks. That keeps the rule the API rests on, which is that a
//! contact reaches a product only because the user chose it.

use std::sync::Arc;

use truapi::{
	latest::GenericError,
	platform::{
		ContactsPlatform, HostContactLookup, HostContactMatches, HostContactPick, ProductContext,
		async_trait,
	},
};

use crate::platform::CliPlatform;

/// One contact as this host stores it: the name it draws, and the account the
/// core is told about when the user picks it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HostContact {
	account: [u8; 32],
	display_name: Option<String>,
}

/// Contacts this host serves, and the surface it asks the user on.
pub struct CliContactsHost {
	contacts: Vec<HostContact>,
	platform: Arc<CliPlatform>,
}

impl CliContactsHost {
	/// Build a contacts host from `TRUAPI_CONTACTS`, falling back to the
	/// development list.
	pub fn from_env(platform: Arc<CliPlatform>) -> Arc<Self> {
		let contacts = match std::env::var("TRUAPI_CONTACTS") {
			Ok(spec) => parse_contacts(&spec),
			Err(_) => development_contacts(),
		};
		Arc::new(Self { contacts, platform })
	}

	/// The contact the picker offers: the one `TRUAPI_CONTACT_PICK` names, or
	/// the first. A CLI has no overlay to choose in, so the choice is
	/// configuration and the user's part is approving it.
	fn offered(&self) -> Option<&HostContact> {
		match std::env::var("TRUAPI_CONTACT_PICK") {
			Ok(name) => self
				.contacts
				.iter()
				.find(|contact| contact.display_name.as_deref() == Some(name.trim())),
			Err(_) => self.contacts.first(),
		}
	}
}

/// Parse `name=0x<hex>;name2=0x<hex>` into contacts, dropping entries whose
/// account is not 32 bytes.
fn parse_contacts(spec: &str) -> Vec<HostContact> {
	spec.split(';')
		.map(str::trim)
		.filter(|entry| !entry.is_empty())
		.filter_map(|entry| {
			let (name, account) = entry.split_once('=')?;
			let account = hex::decode(account.trim().trim_start_matches("0x")).ok()?;
			Some(HostContact {
				account: account.try_into().ok()?,
				display_name: Some(name.trim().to_string()),
			})
		})
		.collect()
}

/// Two contacts with accounts no chain issues, so a development run cannot pay
/// anyone real by accident.
fn development_contacts() -> Vec<HostContact> {
	[("alice", 0xA1u8), ("bob", 0xB0)]
		.into_iter()
		.map(|(name, byte)| HostContact {
			account: [byte; 32],
			display_name: Some(name.to_string()),
		})
		.collect()
}

#[async_trait]
impl ContactsPlatform for CliContactsHost {
	async fn contacts(
		&self,
		lookup: &HostContactLookup,
	) -> Result<HostContactMatches, GenericError> {
		// A handful of configured contacts, so hashing each per lookup is fine;
		// a host with a real store would keep the hash as an indexed column.
		let accounts = lookup
			.handles
			.iter()
			.map(|handle| {
				self.contacts
					.iter()
					.map(|contact| contact.account)
					.find(|account| &truapi::contact_handle(&lookup.handle_key, account) == handle)
			})
			.collect();
		Ok(HostContactMatches { accounts })
	}

	async fn pick_contact(
		&self,
		product: &ProductContext,
	) -> Result<HostContactPick, GenericError> {
		if self.contacts.is_empty() {
			return Ok(HostContactPick::NoContacts);
		}
		let Some(contact) = self.offered() else {
			return Ok(HostContactPick::Dismissed);
		};
		let name = contact.display_name.as_deref().unwrap_or("an unnamed contact");
		let detail = format!(
			"Product {} asked for a contact. Picking {name} hands it a handle for that person, \
             and no name or account.",
			product.product_id,
		);
		if self.platform.decide("pick contact", detail).await {
			Ok(HostContactPick::Picked { account: contact.account })
		} else {
			Ok(HostContactPick::Dismissed)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_spec_names_a_contact_and_its_account() {
		let contacts = parse_contacts(
			"alice=0x0101010101010101010101010101010101010101010101010101010101010101",
		);
		assert_eq!(contacts.len(), 1);
		assert_eq!(contacts[0].display_name.as_deref(), Some("alice"));
		assert_eq!(contacts[0].account, [1u8; 32]);
	}

	#[test]
	fn an_account_that_is_not_32_bytes_is_not_a_contact() {
		// Dropped rather than padded: a short account is a typo, and padding it
		// would mint a handle for somebody who does not exist.
		assert!(parse_contacts("alice=0x0102").is_empty());
		assert!(parse_contacts("alice=not-hex").is_empty());
		assert!(parse_contacts("no-account").is_empty());
	}

	#[test]
	fn an_empty_spec_is_an_empty_list_rather_than_the_development_one() {
		assert!(parse_contacts("").is_empty());
		assert_eq!(development_contacts().len(), 2);
	}

	#[test]
	fn the_development_accounts_are_distinct() {
		let contacts = development_contacts();
		assert_ne!(contacts[0].account, contacts[1].account);
	}
}
