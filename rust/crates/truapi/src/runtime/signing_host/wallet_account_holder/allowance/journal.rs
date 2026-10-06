use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;

use crate::runtime::AuthorityError;
use crate::runtime::statement_allowance::collection::PersonhoodCollection;
use crate::store::{Db, DbError};

/// Only locally confirmed claims establish journal ownership.
#[derive(Clone, Copy)]
pub enum Allocation {
    /// Exact capacity claimed in one personhood collection.
    StatementStore {
        /// Collection whose alias was spent.
        collection: PersonhoodCollection,
        /// Period verified by the registration.
        period: u32,
        /// Slot verified by the registration.
        slot: u32,
    },
    /// A claim verified on Asset Hub.
    SmartContract {
        /// Asset Hub's claim day.
        day: u32,
    },
    /// Authorization observed on Bulletin after a local claim.
    Bulletin,
}

/// Public confirmation remains attributed to its original wallet.
#[derive(Clone, Copy)]
pub struct ConfirmedAllocation {
    wallet: [u8; 32],
    chain: [u8; 32],
    account: [u8; 32],
    allocation: Allocation,
    allocated_at: i64,
}

impl ConfirmedAllocation {
    /// Capture the confirmation time once, including for receipt replay.
    pub fn new(
        wallet: [u8; 32],
        chain: [u8; 32],
        account: [u8; 32],
        allocation: Allocation,
    ) -> Result<Self, AuthorityError> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| AuthorityError::Unavailable {
                reason: error.to_string(),
            })?;
        let allocated_at =
            duration
                .as_millis()
                .try_into()
                .map_err(
                    |error: core::num::TryFromIntError| AuthorityError::Unavailable {
                        reason: error.to_string(),
                    },
                )?;
        Ok(Self {
            wallet,
            chain,
            account,
            allocation,
            allocated_at,
        })
    }

    /// Commit account history and any owned slot together.
    pub async fn record(&self, database: &Db) -> Result<(), DbError> {
        let Self {
            wallet,
            chain,
            account,
            allocation,
            allocated_at,
        } = *self;
        database.write(move |transaction| {
            let (resource, priority, period) = match allocation {
                Allocation::StatementStore { .. } => ("statement-store-allowance", None, None),
                Allocation::SmartContract { day } => ("smart-contract-allowance", Some(0), Some(day)),
                Allocation::Bulletin => ("bulletin-allowance", Some(0), None),
            };
            transaction.execute(
                "INSERT INTO allowance_records (wallet, chain, resource, account, allocated_at, priority, last_renewed_period)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(wallet, chain, resource, account) DO UPDATE SET
                    allocated_at = MAX(allowance_records.allocated_at, excluded.allocated_at),
                    priority = excluded.priority,
                    last_renewed_period = CASE WHEN excluded.last_renewed_period IS NULL THEN NULL
                        ELSE MAX(COALESCE(allowance_records.last_renewed_period, excluded.last_renewed_period), excluded.last_renewed_period) END",
                params![wallet.as_slice(), chain.as_slice(), resource, account.as_slice(), allocated_at, priority, period],
            )?;
            if let Allocation::StatementStore { collection, period, slot } = allocation {
                transaction.execute(
                    "INSERT INTO statement_slots (wallet, chain, collection, period, slot, account, priority, last_allocated_or_renewed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
                     ON CONFLICT(wallet, chain, collection, period, slot) DO UPDATE SET
                        account = excluded.account,
                        priority = excluded.priority,
                        last_allocated_or_renewed_at = excluded.last_allocated_or_renewed_at",
                    params![wallet.as_slice(), chain.as_slice(), collection.metadata_variant(), period, slot, account.as_slice(), allocated_at],
                )?;
            }
            Ok(())
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn receipts_preserve_each_owned_slot_and_history_after_reopen() {
        block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let database = Db::open(crate::store::core_db_config(directory.path()))
                .await
                .unwrap();
            let first = ConfirmedAllocation {
                wallet: [1; 32],
                chain: [2; 32],
                account: [3; 32],
                allocation: Allocation::StatementStore {
                    collection: PersonhoodCollection::People,
                    period: 7,
                    slot: 0,
                },
                allocated_at: 1_000,
            };
            let receipts = [
                first,
                ConfirmedAllocation {
                    allocation: Allocation::StatementStore {
                        collection: PersonhoodCollection::People,
                        period: 7,
                        slot: 1,
                    },
                    allocated_at: 2_000,
                    ..first
                },
                ConfirmedAllocation {
                    allocation: Allocation::StatementStore {
                        collection: PersonhoodCollection::LitePeople,
                        period: 7,
                        slot: 0,
                    },
                    ..first
                },
                ConfirmedAllocation {
                    wallet: [4; 32],
                    ..first
                },
                ConfirmedAllocation {
                    chain: [5; 32],
                    ..first
                },
                ConfirmedAllocation {
                    allocation: Allocation::SmartContract { day: 12 },
                    ..first
                },
                ConfirmedAllocation {
                    allocation: Allocation::SmartContract { day: 13 },
                    allocated_at: 500,
                    ..first
                },
                ConfirmedAllocation {
                    allocation: Allocation::Bulletin,
                    ..first
                },
                first,
            ];
            for receipt in receipts {
                receipt.record(&database).await.unwrap();
            }
            database.close().await.unwrap();
            let reopened = Db::open(crate::store::core_db_config(directory.path()))
                .await
                .unwrap();
            let records = reopened.read(|connection| {
                let accounts = connection.prepare("SELECT wallet, chain, resource, account, allocated_at, priority, last_renewed_period FROM allowance_records ORDER BY wallet, chain, resource")?
                    .query_map([], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, String>(2)?, row.get::<_, Vec<u8>>(3)?, row.get::<_, i64>(4)?, row.get::<_, Option<i64>>(5)?, row.get::<_, Option<u32>>(6)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                let slots = connection.prepare("SELECT wallet, chain, collection, period, slot, account, priority, last_allocated_or_renewed_at FROM statement_slots ORDER BY wallet, chain, collection, slot")?
                    .query_map([], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, String>(2)?, row.get::<_, u32>(3)?, row.get::<_, u32>(4)?, row.get::<_, Vec<u8>>(5)?, row.get::<_, u32>(6)?, row.get::<_, i64>(7)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((accounts, slots))
            }).await.unwrap();
            let account = |wallet, chain, resource: &str, time, priority, period| {
                (
                    vec![wallet; 32],
                    vec![chain; 32],
                    resource.to_string(),
                    vec![3; 32],
                    time,
                    priority,
                    period,
                )
            };
            let slot = |wallet, chain, collection: &str, slot, time| {
                (
                    vec![wallet; 32],
                    vec![chain; 32],
                    collection.to_string(),
                    7,
                    slot,
                    vec![3; 32],
                    0,
                    time,
                )
            };
            assert_eq!(
                records,
                (
                    vec![
                        account(1, 2, "bulletin-allowance", 1_000, Some(0), None),
                        account(1, 2, "smart-contract-allowance", 1_000, Some(0), Some(13)),
                        account(1, 2, "statement-store-allowance", 2_000, None, None),
                        account(1, 5, "statement-store-allowance", 1_000, None, None),
                        account(4, 2, "statement-store-allowance", 1_000, None, None),
                    ],
                    vec![
                        slot(1, 2, "LitePeople", 0, 1_000),
                        slot(1, 2, "People", 0, 1_000),
                        slot(1, 2, "People", 1, 2_000),
                        slot(1, 5, "People", 0, 1_000),
                        slot(4, 2, "People", 0, 1_000),
                    ],
                )
            );
        });
    }
}
