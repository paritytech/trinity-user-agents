//! Read-only snapshot of a product's public resource state.
//!
//! Derives the product's public accounts from the active session and reads
//! what each chain publicly holds for them, pinned to the finalized block of
//! each chain. Nothing here allocates, renews, claims, registers, signs or
//! builds a proof: the ring-VRF alias behind an allowance is never derived, so
//! the statement-store reading goes through the public reverse index from the
//! allowance account instead.

use serde::Serialize;
use truapi::latest::ChainIdentifier;
use truapi::v01;

use super::{AuthorityError, SigningHost, product_authority_error};
use crate::host_logic::features;
use crate::host_logic::product_account::derive_sr25519_hard_path;
use crate::platform::normalize_product_identifier;
use crate::runtime::RuntimeServices;
use crate::runtime::statement_allowance::rpc::RpcClient;
use crate::runtime::statement_allowance::{
    self, ChainClient, fetch_bulletin_allowance_at, pgas, slot,
};

/// The account index a product's PGAS balance is read for.
const PRODUCT_ACCOUNT_INDEX: u32 = 0;

/// One part of the snapshot: what the chain holds, or why it could not be read.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ResourceReading<T> {
    /// The chain answered. An empty answer is inside `T`, never here.
    Read(T),
    /// The read failed, or the runtime does not have what the read needs.
    Unavailable {
        /// The public account the read was for.
        account: String,
        /// Why nothing was read.
        reason: String,
    },
}

/// How an entry of the reverse index relates to the chain's current period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AllowanceEntryState {
    /// The entry is for the period the chain is in.
    Current,
    /// An earlier period still inside the runtime's grace window.
    Grace,
    /// An earlier period past the grace window, stored until cleanup.
    Lapsed,
    /// An earlier period, with no grace window read to place it.
    Earlier,
    /// A period after the chain's current one.
    Ahead,
}

/// One `Resources.StmtStoreAllowanceByAccount` entry.
#[derive(Debug, Serialize)]
pub struct AllowanceEntry {
    /// The period the entry belongs to.
    pub period: u32,
    /// The slot within that period.
    pub seq: u32,
    /// Where the period stands against the chain's clock.
    pub state: AllowanceEntryState,
}

/// What the People chain holds for the product's statement-store account.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementStoreReading {
    /// The allowance account, hex.
    pub account: String,
    /// The finalized block the read is pinned to.
    pub block_hash: String,
    /// The period the chain's clock is in at that block.
    pub current_period: u32,
    /// The runtime's grace window in seconds, when it could be read.
    pub grace_seconds: Option<u32>,
    /// Every entry of the account in the reverse index; empty when none.
    pub entries: Vec<AllowanceEntry>,
}

/// A live `TransactionStorage.Authorizations` record.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulletinAuthorization {
    /// Transactions the authorization still allows.
    pub remaining_transactions: u32,
    /// Bytes the authorization still allows, decimal.
    pub remaining_bytes: String,
    /// The block at which the authorization expires.
    pub expires_at_block: u32,
    /// Whether the block the read is pinned to is at or past that block.
    pub expired: bool,
}

/// What Bulletin holds for the product's Bulletin allowance account.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulletinReading {
    /// The allowance account, hex.
    pub account: String,
    /// The finalized block the read is pinned to.
    pub block_hash: String,
    /// That block's number.
    pub block_number: u32,
    /// The record, or `None` when the account has no authorization.
    pub authorization: Option<BulletinAuthorization>,
}

/// The PGAS balance of the product's account on Asset Hub.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgasReading {
    /// The product account, hex.
    pub account: String,
    /// The account index within the product.
    pub account_index: u32,
    /// The finalized block the read is pinned to.
    pub block_hash: String,
    /// The runtime's PGAS asset id.
    pub asset_id: u32,
    /// Balance in the smallest unit, decimal; zero when the account has no entry.
    pub balance: String,
    /// The amount one claim mints, in the smallest unit, decimal.
    pub claim_amount: String,
    /// Decimal places of one whole unit, when the runtime holds asset metadata.
    pub decimals: Option<u8>,
    /// Ticker symbol, when the runtime holds asset metadata.
    pub symbol: Option<String>,
}

/// The public resource state of one product for the active session.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductResourceStatus {
    /// The normalized product id the snapshot is for.
    pub product_id: String,
    /// The product's Statement Store allocation on the People chain.
    pub statement_store: ResourceReading<StatementStoreReading>,
    /// The product's Bulletin authorization.
    pub bulletin: ResourceReading<BulletinReading>,
    /// The product account's PGAS balance on Asset Hub.
    pub pgas: ResourceReading<PgasReading>,
}

/// The public accounts a product's resources are held under.
struct ResourceAccounts {
    statement_store: [u8; 32],
    bulletin: [u8; 32],
    product_account: [u8; 32],
}

fn hex_id(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn reading<T>(account: &[u8; 32], result: Result<T, String>) -> ResourceReading<T> {
    match result {
        Ok(value) => ResourceReading::Read(value),
        Err(reason) => ResourceReading::Unavailable {
            account: hex_id(account),
            reason,
        },
    }
}

fn entry_state(
    period: u32,
    current_period: u32,
    now_seconds: u64,
    grace_seconds: Option<u32>,
) -> AllowanceEntryState {
    match (period.cmp(&current_period), grace_seconds) {
        (core::cmp::Ordering::Equal, _) => AllowanceEntryState::Current,
        (core::cmp::Ordering::Greater, _) => AllowanceEntryState::Ahead,
        (core::cmp::Ordering::Less, None) => AllowanceEntryState::Earlier,
        (core::cmp::Ordering::Less, Some(grace)) => {
            let period_end = (u64::from(period) + 1) * slot::STATEMENT_STORE_PERIOD_SECONDS;
            if now_seconds < period_end + u64::from(grace) {
                AllowanceEntryState::Grace
            } else {
                AllowanceEntryState::Lapsed
            }
        }
    }
}

async fn read_statement_store(
    services: &RuntimeServices,
    account: [u8; 32],
) -> Result<StatementStoreReading, String> {
    let client = services
        .statement_store
        .chain_client("product resource status")
        .await
        .map_err(|err| err.to_string())?;
    let rpc = client.rpc();
    let chain = services
        .chain_context
        .get(&client)
        .await
        .map_err(|err| err.to_string())?;
    let block_hash = rpc.finalized_head().await.map_err(|err| err.to_string())?;
    let entries = slot::read_account_allowances_at(rpc, &chain.metadata, &account, &block_hash)
        .await
        .map_err(|err| err.to_string())?;
    let now_seconds = slot::read_chain_now_seconds_at(rpc, &block_hash)
        .await
        .map_err(|err| err.to_string())?;
    let current_period = slot::current_period(now_seconds);
    let grace_seconds = slot::statement_store_grace_window(rpc, &chain.metadata)
        .await
        .ok();
    Ok(StatementStoreReading {
        account: hex_id(&account),
        block_hash,
        current_period,
        grace_seconds,
        entries: entries
            .into_iter()
            .map(|entry| AllowanceEntry {
                period: entry.period,
                seq: entry.seq,
                state: entry_state(entry.period, current_period, now_seconds, grace_seconds),
            })
            .collect(),
    })
}

async fn read_bulletin(
    services: &RuntimeServices,
    account: [u8; 32],
) -> Result<BulletinReading, String> {
    let rpc = RpcClient::new(
        services
            .bulletin
            .client("product resource status")
            .await
            .map_err(|err| err.to_string())?,
    );
    let block_hash = rpc.finalized_head().await.map_err(|err| err.to_string())?;
    let authorization = fetch_bulletin_allowance_at(&rpc, &account, &block_hash)
        .await
        .map_err(|err| err.to_string())?;
    // `fetched_at` is the pinned block's number, so a missing record needs it
    // fetched on its own.
    let block_number = match &authorization {
        Some(info) => info.fetched_at,
        None => statement_allowance::fetch_block_number_at(&rpc, &block_hash)
            .await
            .map_err(|err| err.to_string())?,
    };
    Ok(BulletinReading {
        account: hex_id(&account),
        block_hash,
        block_number,
        authorization: authorization.map(|info| BulletinAuthorization {
            remaining_transactions: info.remained_transactions,
            remaining_bytes: info.remained_size.to_string(),
            expires_at_block: info.expires_in,
            expired: info.fetched_at >= info.expires_in,
        }),
    })
}

async fn read_pgas(services: &RuntimeServices, account: [u8; 32]) -> Result<PgasReading, String> {
    let chains = features::supported_chains(services.platform.as_ref())
        .await
        .map_err(|err| err.reason)?;
    let genesis = features::genesis_for(&chains, ChainIdentifier::AssetHub)
        .ok_or("this host does not serve Asset Hub")?;
    let client = ChainClient::new(
        RpcClient::new(subxt_rpcs::RpcClient::new(
            services
                .chain
                .rpc_client("product resource status", &genesis)
                .await
                .map_err(|err| err.to_string())?,
        )),
        genesis,
    );
    let rpc = client.rpc();
    let chain = services
        .chain_context
        .get(&client)
        .await
        .map_err(|err| err.to_string())?;
    let block_hash = rpc.finalized_head().await.map_err(|err| err.to_string())?;
    let balance = pgas::read_balance_at(rpc, &chain.metadata, &account, &block_hash)
        .await
        .map_err(|err| err.to_string())?;
    Ok(PgasReading {
        account: hex_id(&account),
        account_index: PRODUCT_ACCOUNT_INDEX,
        block_hash,
        asset_id: balance.asset_id,
        balance: balance.balance.to_string(),
        claim_amount: balance.claim_amount.to_string(),
        decimals: balance.display.as_ref().map(|display| display.decimals),
        symbol: balance.display.and_then(|display| display.symbol),
    })
}

impl SigningHost {
    /// The public accounts `product_id`'s resources are held under, derived
    /// from the active session's entropy. Only public keys leave this.
    fn resource_accounts(&self, product_id: &str) -> Result<ResourceAccounts, AuthorityError> {
        let entropy = self.root_entropy()?;
        let public_of = |junctions: &[&str]| {
            derive_sr25519_hard_path(&entropy, junctions)
                .map(|keypair| keypair.public.to_bytes())
                .map_err(product_authority_error)
        };
        Ok(ResourceAccounts {
            statement_store: public_of(&["allowance", "statement-store", product_id])?,
            bulletin: public_of(&["allowance", "bulletin", product_id])?,
            product_account: self
                .product_keypair(&v01::ProductAccountId {
                    dot_ns_identifier: product_id.to_string(),
                    derivation_index: v01::DerivationIndex::Index(PRODUCT_ACCOUNT_INDEX),
                })?
                .public
                .to_bytes(),
        })
    }

    /// Read what the chains publicly hold for `product_id` under the active
    /// session, without changing anything.
    ///
    /// The three readings are independent: one chain failing leaves the others
    /// intact. Fails with [`AuthorityError::Disconnected`] when no session is
    /// active, or when the session changed while the chains were read, so a
    /// reply never describes a wallet that is no longer signed in.
    pub async fn product_resource_status(
        &self,
        product_id: &str,
    ) -> Result<ProductResourceStatus, AuthorityError> {
        let product_id = normalize_product_identifier(product_id).map_err(|err| {
            AuthorityError::Unavailable {
                reason: err.to_string(),
            }
        })?;
        let session = self
            .current_local_session()
            .ok_or(AuthorityError::Disconnected)?;
        let accounts = self.resource_accounts(&product_id)?;
        let services = &self.services;
        let (statement_store, bulletin, pgas) = futures::join!(
            read_statement_store(services, accounts.statement_store),
            read_bulletin(services, accounts.bulletin),
            read_pgas(services, accounts.product_account),
        );
        self.require_current_session(&session)?;
        Ok(ProductResourceStatus {
            product_id,
            statement_store: reading(&accounts.statement_store, statement_store),
            bulletin: reading(&accounts.bulletin, bulletin),
            pgas: reading(&accounts.product_account, pgas),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = slot::STATEMENT_STORE_PERIOD_SECONDS;

    #[test]
    fn an_entry_for_the_chains_period_is_current() {
        assert_eq!(
            entry_state(10, 10, 10 * DAY + 5, Some(3_600)),
            AllowanceEntryState::Current,
        );
    }

    /// An ended period keeps its allowance only until the grace window closes,
    /// counted from the end of that period, not from its start.
    #[test]
    fn an_earlier_period_holds_until_its_grace_window_closes() {
        let period_end = 10 * DAY;
        assert_eq!(
            entry_state(9, 10, period_end + 3_599, Some(3_600)),
            AllowanceEntryState::Grace,
        );
        assert_eq!(
            entry_state(9, 10, period_end + 3_600, Some(3_600)),
            AllowanceEntryState::Lapsed,
        );
    }

    #[test]
    fn an_earlier_period_with_no_grace_window_read_is_not_placed() {
        assert_eq!(
            entry_state(9, 10, 10 * DAY, None),
            AllowanceEntryState::Earlier,
        );
        assert_eq!(
            entry_state(11, 10, 10 * DAY, Some(3_600)),
            AllowanceEntryState::Ahead,
        );
    }

    /// The page parses this document, so the field names and the tag are its
    /// contract. Amounts travel as decimal text so a balance past 2^53 is exact.
    #[test]
    fn the_document_keeps_a_failed_part_apart_from_an_empty_one() {
        let status = ProductResourceStatus {
            product_id: "demo.paseo".into(),
            statement_store: ResourceReading::Read(StatementStoreReading {
                account: "0x01".into(),
                block_hash: "0x02".into(),
                current_period: 7,
                grace_seconds: None,
                entries: vec![],
            }),
            bulletin: ResourceReading::Unavailable {
                account: "0x03".into(),
                reason: "no route".into(),
            },
            pgas: ResourceReading::Read(PgasReading {
                account: "0x04".into(),
                account_index: 0,
                block_hash: "0x05".into(),
                asset_id: 9,
                balance: u128::MAX.to_string(),
                claim_amount: "50".into(),
                decimals: Some(10),
                symbol: None,
            }),
        };

        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            serde_json::json!({
                "productId": "demo.paseo",
                "statementStore": {
                    "status": "read",
                    "account": "0x01",
                    "blockHash": "0x02",
                    "currentPeriod": 7,
                    "graceSeconds": null,
                    "entries": [],
                },
                "bulletin": {
                    "status": "unavailable",
                    "account": "0x03",
                    "reason": "no route",
                },
                "pgas": {
                    "status": "read",
                    "account": "0x04",
                    "accountIndex": 0,
                    "blockHash": "0x05",
                    "assetId": 9,
                    "balance": u128::MAX.to_string(),
                    "claimAmount": "50",
                    "decimals": 10,
                    "symbol": null,
                },
            }),
        );
    }
}
