//! `/search`: the identity backend's usernames under a prefix, each checked against
//! what Asset Hub records for it.
//!
//! [`search`] is the entry point. The backend's assignment and the chain's records
//! can disagree, and apps only show names the chain's gateway records.

use anyhow::{Context, Result};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use truapi::host_logic::product_account::public_key_from_address;

use crate::{
	attestation::{self, UsernameSearchItem},
	dotns_read::{AccountNameRecord, AssetHubReader, NameEntry},
	network::Stack,
};

/// Most rows a listing checks and prints before asking for a narrower prefix.
const DISPLAY_LIMIT: usize = 50;

/// Searches the backend for `prefix`, reads each listed row's Asset Hub records,
/// and renders them as a table.
///
/// Backend, address decoding and chain read failures return errors.
pub async fn search(network: &Stack, prefix: &str) -> Result<String> {
	const CONCURRENT_ROWS: usize = 8;

	let matches = attestation::search_usernames(network, prefix).await?;
	if matches.is_empty() {
		return Ok(format_rows(prefix, &[], 0));
	}
	let hidden = matches.len().saturating_sub(DISPLAY_LIMIT);
	let reader = AssetHubReader::connect(network).await?;
	let rows: Vec<SearchRow> = stream::iter(matches.into_iter().take(DISPLAY_LIMIT))
		.map(|found| {
			let mut reader = reader.clone();
			async move {
				let chain = ChainRecord::read(&mut reader, &found).await?;
				anyhow::Ok(SearchRow { found, chain })
			}
		})
		.buffered(CONCURRENT_ROWS)
		.try_collect()
		.await?;
	Ok(format_rows(prefix, &rows, hidden))
}

struct SearchRow {
	found: UsernameSearchItem,
	chain: ChainRecord,
}

/// What Asset Hub records for one backend row's name and account.
struct ChainRecord {
	account: [u8; 32],
	/// `DotnsGateway.LiteLabelOwner` of the row's name.
	label_owner: Option<[u8; 32]>,
	/// `DotnsGateway.AccountNames` of the row's account, which apps search through.
	account_names: Option<AccountNameRecord>,
	/// Username the dotNS contracts resolve for the account, as the core reads it.
	contracts_username: Option<String>,
}

impl ChainRecord {
	async fn read(reader: &mut AssetHubReader, found: &UsernameSearchItem) -> Result<Self> {
		let account = public_key_from_address(&found.account_id)
			.with_context(|| format!("backend account {:?} is not an address", found.account_id))?;
		Ok(Self {
			account,
			label_owner: reader.lite_label_owner(&found.username).await?,
			account_names: reader.account_names(&account).await?,
			contracts_username: reader
				.dotns_identity(&account)
				.await?
				.map(|identity| identity.primary_username().to_owned()),
		})
	}

	fn label_owner_cell(&self) -> &'static str {
		match self.label_owner {
			Some(owner) if owner == self.account => "matches",
			Some(_) => "other account",
			None => "none",
		}
	}

	fn account_names_cell(&self) -> String {
		let entries: Vec<&NameEntry> = self
			.account_names
			.iter()
			.flat_map(|record| [&record.lite, &record.full])
			.flatten()
			.collect();
		if entries.is_empty() {
			return "none".to_string();
		}
		let labels = entries
			.iter()
			.map(|entry| String::from_utf8_lossy(&entry.label))
			.collect::<Vec<_>>()
			.join(", ");
		if entries.iter().any(|entry| entry.chat.is_some()) {
			labels
		} else {
			format!("{labels} (no chat key)")
		}
	}
}

fn format_rows(prefix: &str, rows: &[SearchRow], hidden: usize) -> String {
	if rows.is_empty() {
		return format!("No username starts with {prefix:?}.");
	}
	let header = ["name", "backend", "account", "LiteLabelOwner", "AccountNames", "contracts"]
		.map(String::from);
	let cells: Vec<[String; 6]> = core::iter::once(header)
		.chain(rows.iter().map(|row| {
			[
				row.found.username.clone(),
				row.found.status.clone(),
				row.found.account_id.clone(),
				row.chain.label_owner_cell().to_string(),
				row.chain.account_names_cell(),
				row.chain.contracts_username.clone().unwrap_or_else(|| "none".to_string()),
			]
		}))
		.collect();
	let widths: [usize; 6] = core::array::from_fn(|column| {
		cells.iter().map(|line| line[column].len()).max().unwrap_or(0)
	});
	let mut lines: Vec<String> = cells
		.iter()
		.map(|line| {
			let (last, padded) = line.split_last().expect("six columns");
			let padded: Vec<String> = padded
				.iter()
				.zip(widths)
				.map(|(cell, width)| format!("{cell:<width$}"))
				.collect();
			format!("{}  {last}", padded.join("  "))
		})
		.collect();
	if hidden > 0 {
		lines.push(format!("… and {hidden} more; narrow the prefix"));
	}
	lines.join("\n")
}

#[cfg(test)]
mod tests;
