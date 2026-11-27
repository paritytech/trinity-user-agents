use super::*;

fn found(username: &str, account_id: &str) -> UsernameSearchItem {
	UsernameSearchItem {
		account_id: account_id.to_string(),
		username: username.to_string(),
		status: "ASSIGNED".to_string(),
	}
}

fn lite_entry(label: &str, chat: Option<[u8; 65]>) -> Option<NameEntry> {
	Some(NameEntry { label: label.as_bytes().to_vec(), chat })
}

/// Contacts in the app come from the backend but survive only when Asset Hub's
/// gateway records the same name with a chat key. The listing shows each source
/// side by side, so a name the backend assigned but the chain never recorded
/// stands out.
#[test]
fn listing_shows_backend_and_chain_records_side_by_side() {
	let rows = [
		SearchRow {
			found: found("schtrule.01", "5Sch1"),
			chain: ChainRecord {
				account: [1; 32],
				label_owner: Some([1; 32]),
				account_names: Some(AccountNameRecord {
					lite: lite_entry("schtrule.01", Some([7; 65])),
					full: None,
				}),
				contracts_username: None,
			},
		},
		SearchRow {
			found: found("schtrule.04", "5Sch4"),
			chain: ChainRecord {
				account: [4; 32],
				label_owner: None,
				account_names: None,
				contracts_username: None,
			},
		},
		SearchRow {
			found: found("other.01", "5Other"),
			chain: ChainRecord {
				account: [9; 32],
				label_owner: Some([8; 32]),
				account_names: Some(AccountNameRecord {
					lite: lite_entry("other.01", None),
					full: None,
				}),
				contracts_username: Some("other.01".to_string()),
			},
		},
	];

	assert_eq!(
		format_rows("s", &rows, 2),
		[
			"identity backend                Asset Hub: gateway pallet               Asset Hub: dotNS contracts",
			"name         status    account  LiteLabelOwner  AccountNames            username",
			"schtrule.01  ASSIGNED  5Sch1    matches         schtrule.01             none",
			"schtrule.04  ASSIGNED  5Sch4    none            none                    none",
			"other.01     ASSIGNED  5Other   other account   other.01 (no chat key)  other.01",
			"… and 2 more; narrow the prefix",
		]
		.join("\n")
	);
	assert_eq!(format_rows("zz", &[], 0), "No username starts with \"zz\".");
}
