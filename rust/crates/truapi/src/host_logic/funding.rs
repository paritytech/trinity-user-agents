//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death through
//! [`CoreStorageKey::FundingSessions`], and is re-attached by
//! `Funding::status_subscribe`. A session always terminates, because the core
//! expires it on its own clock.

use std::collections::BTreeMap;

use parity_scale_codec::{Decode, Encode};
use tracing::warn;
use truapi::latest::{FundingDirection, FundingFailure, HostFundingStatusSubscribeItem};

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many settled sessions the core keeps, most recently settled first.
const SETTLED_HISTORY_LIMIT: usize = 50;

/// What the core knows about one session, independent of any host surface.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FundingSession {
    /// Identifier handed back to the caller and used to re-attach.
    pub intent: String,
    /// Product that opened the session and the only one that may watch it.
    /// `None` when the host opened it, as the Balance card does.
    pub owner_product_id: Option<String>,
    /// Which way value crosses the boundary.
    pub direction: FundingDirection,
    /// Amount sought, in the user's payment balance units. `None` until the
    /// user chooses one.
    pub amount: Option<u128>,
    /// Current stage.
    pub stage: FundingStage,
    /// When the session was opened, in Unix milliseconds.
    pub opened_at_ms: u64,
    /// When the session expires if still open, in Unix milliseconds.
    pub deadline_ms: u64,
    /// Where an inbound session's provider delivers, once the source is
    /// known.
    pub deposit: Option<FundingDeposit>,
}

/// Asset Hub asset a deposit arrives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum DepositAsset {
    /// The relay chain's native token.
    Native,
    /// An `Assets` pallet asset.
    Asset(u32),
}

/// What an inbound session's provider delivers, once it is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositRequest {
    /// Deposit source, as in the account label, such as `usdt-assethub`.
    pub source_id: String,
    /// Asset the provider delivers.
    pub asset: DepositAsset,
    /// Balance at which the deposit counts as delivered, in `asset` units.
    pub expected: u128,
}

/// The account an inbound session watches and what it waits for.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FundingDeposit {
    /// Deposit source, as in the account label.
    pub source_id: String,
    /// Account number for `source_id`; the refund account shares it.
    pub number: u32,
    /// Asset the provider delivers.
    pub asset: DepositAsset,
    /// Public key of the deposit account, kept so the watch needs no signing
    /// session.
    pub account: [u8; 32],
    /// Balance at which the deposit counts as delivered, in `asset` units.
    pub expected: u128,
    /// How the deposit becomes CASH on People.
    pub route: ConversionRoute,
}

/// How a deposit becomes CASH on People, fixed when its account is assigned
/// so a later change on chain cannot switch it mid-session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ConversionRoute {
    /// The deposit is CASH already: teleport it.
    Teleport,
    /// The deposit is a stablecoin the PSM mints CASH against: mint, then
    /// teleport.
    Psm {
        /// Minting fee the route was chosen at, in parts per million.
        fee_ppm: u32,
    },
}

/// A conversion transaction handed to Asset Hub, with what tells whether it
/// worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ConversionSubmission {
    /// The deposit account's nonce the transaction was signed at.
    pub nonce: u32,
    /// When it was submitted, in Unix milliseconds.
    pub submitted_at_ms: u64,
    /// Last Asset Hub block its mortal era admits it in.
    pub valid_until_block: u64,
    /// CASH the account held on People before it was submitted.
    pub people_before: u128,
    /// Least CASH the conversion lands on People.
    pub landing: u128,
    /// Deposit the transaction takes from the account on Asset Hub.
    pub spent: u128,
}

/// Dry runs refused before a conversion gives up.
const MAX_CONVERSION_REFUSALS: u8 = 3;

/// Stage of a session, as the core persists it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FundingStage {
    /// In flight.
    Open,
    /// Inbound: the deposit arrived and is being converted.
    Converting {
        /// Balance of the deposit account when it was seen, in its asset's
        /// units.
        deposited: u128,
        /// Dry runs the chain has refused so far.
        refusals: u8,
        /// The conversion transaction, once one is on its way.
        submission: Option<ConversionSubmission>,
    },
    /// Inbound: CASH landed on the deposit account on People and awaits
    /// crediting.
    Converted {
        /// CASH on People, in payment balance units.
        landed: u128,
    },
    /// Ended without success.
    Failed {
        /// Why it ended.
        reason: FundingFailure,
        /// When it ended, in Unix milliseconds.
        settled_at_ms: u64,
    },
}

impl FundingSession {
    /// Open a session that expires one window from `now_ms`.
    pub fn new(
        intent: String,
        owner_product_id: Option<String>,
        direction: FundingDirection,
        amount: Option<u128>,
        now_ms: u64,
    ) -> Self {
        Self {
            intent,
            owner_product_id,
            direction,
            amount,
            stage: FundingStage::Open,
            opened_at_ms: now_ms,
            deadline_ms: now_ms.saturating_add(SESSION_WINDOW_MS),
            deposit: None,
        }
    }

    /// Whether the session has ended.
    pub fn is_terminal(&self) -> bool {
        self.settled_at_ms().is_some()
    }

    /// When the session ended, if it has.
    pub fn settled_at_ms(&self) -> Option<u64> {
        match self.stage {
            FundingStage::Open | FundingStage::Converting { .. } | FundingStage::Converted { .. } => {
                None
            }
            FundingStage::Failed { settled_at_ms, .. } => Some(settled_at_ms),
        }
    }

    /// Project the current stage onto the wire item subscribers receive.
    pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
        match (&self.stage, self.direction) {
            (FundingStage::Open, FundingDirection::In) => {
                HostFundingStatusSubscribeItem::AwaitingDeposit {
                    expires_at: Some(self.deadline_ms),
                }
            }
            (FundingStage::Open, FundingDirection::Out) => {
                HostFundingStatusSubscribeItem::AwaitingRelease
            }
            (FundingStage::Converting { .. } | FundingStage::Converted { .. }, _) => {
                HostFundingStatusSubscribeItem::Converting
            }
            (FundingStage::Failed { reason, .. }, _) => HostFundingStatusSubscribeItem::Failed {
                reason: reason.clone(),
                moved: 0,
            },
        }
    }

    /// End the session with `reason`, unless it already ended. Returns whether
    /// it changed.
    pub fn fail(&mut self, reason: FundingFailure, now_ms: u64) -> bool {
        if self.is_terminal() {
            return false;
        }
        self.stage = FundingStage::Failed {
            reason,
            settled_at_ms: now_ms,
        };
        true
    }

    /// Whether the expiry sweep ends this session at its deadline: an open
    /// one with no deposit account. One with an account is ended by the
    /// deposit watch, after a read that shows its deposit did not arrive.
    pub fn expires_by_sweep(&self) -> bool {
        self.stage == FundingStage::Open && self.deposit.is_none()
    }

    /// Expire the session if the sweep owns it and its deadline passed.
    /// Returns whether it expired.
    pub fn expire_if_due(&mut self, now_ms: u64) -> bool {
        self.expires_by_sweep()
            && now_ms >= self.deadline_ms
            && self.fail(FundingFailure::Expired, now_ms)
    }

    /// The deposit an open inbound session is waiting on, if one is assigned.
    pub fn awaited_deposit(&self) -> Option<&FundingDeposit> {
        (self.stage == FundingStage::Open)
            .then_some(self.deposit.as_ref())
            .flatten()
    }

    /// Record a finalized reading of the deposit account's balance, taken at
    /// `now_ms`: converting once it covers the expected amount, expired if it
    /// does not by the deadline. Returns whether the session changed.
    pub fn observe_deposit(&mut self, balance: u128, now_ms: u64) -> bool {
        let Some(deposit) = self.awaited_deposit() else {
            return false;
        };
        if balance >= deposit.expected {
            self.stage = FundingStage::Converting {
                deposited: balance,
                refusals: 0,
                submission: None,
            };
            return true;
        }
        now_ms >= self.deadline_ms && self.fail(FundingFailure::Expired, now_ms)
    }

    /// The deposit of a session being converted, with its submission so far.
    pub fn converting(&self) -> Option<(&FundingDeposit, Option<ConversionSubmission>)> {
        match (&self.stage, &self.deposit) {
            (FundingStage::Converting { submission, .. }, Some(deposit)) => {
                Some((deposit, *submission))
            }
            _ => None,
        }
    }

    /// Advance a converting session by one step of its conversion. Returns
    /// whether the session changed.
    pub fn advance_conversion(&mut self, step: ConversionStep, now_ms: u64) -> bool {
        let FundingStage::Converting {
            refusals,
            submission,
            ..
        } = &mut self.stage
        else {
            return false;
        };
        match step {
            ConversionStep::Submitted(submitted) => *submission = Some(submitted),
            ConversionStep::Dropped => *submission = None,
            ConversionStep::Refused { reason } => {
                *submission = None;
                *refusals = refusals.saturating_add(1);
                if *refusals >= MAX_CONVERSION_REFUSALS {
                    return self.fail(
                        FundingFailure::Other {
                            code: "conversion_refused".into(),
                            message: reason,
                        },
                        now_ms,
                    );
                }
            }
            ConversionStep::Landed { landed } => {
                self.stage = FundingStage::Converted { landed };
            }
            ConversionStep::Stalled => {
                return self.fail(
                    FundingFailure::Other {
                        code: "conversion_stalled".into(),
                        message: "the conversion left Asset Hub but never reached People".into(),
                    },
                    now_ms,
                );
            }
        }
        true
    }
}

/// What one pass of a conversion found or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionStep {
    /// The conversion transaction is about to be submitted.
    Submitted(ConversionSubmission),
    /// The submitted transaction can no longer convert anything: its era
    /// ended unincluded, or it was included and failed. The next pass
    /// submits again.
    Dropped,
    /// A dry run refused the conversion.
    Refused {
        /// Why, as the chain reported it.
        reason: String,
    },
    /// CASH arrived on People.
    Landed {
        /// CASH on People, in payment balance units.
        landed: u128,
    },
    /// The transaction took the deposit on Asset Hub but its CASH never
    /// reached People.
    Stalled,
}

/// Which of a session's accounts under the reserved funding product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundingAccountKind {
    /// Where an inbound provider delivers.
    Deposit,
    /// Where a crypto rail returns funds it could not deliver.
    Refund,
    /// Where an outbound session stages funds before paying the provider.
    Withdrawal,
}

/// Why a funding account label could not be built.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display("funding account label is longer than 32 bytes")]
pub struct FundingAccountLabelTooLong;

/// Derivation index of the `number`th account of `kind` for `source_id`: the
/// label `onramp:eph:<source>:<n>`, `onramp:rf:<source>:<n>` or
/// `wd:eph:<source>:<n>`, zero-padded to 32 bytes.
///
/// The labels are the ones getcash uses, and `number` counts up from 1 per
/// source, so every account can be found again from the seed alone.
pub fn funding_account_index(
    kind: FundingAccountKind,
    source_id: &str,
    number: u32,
) -> Result<[u8; 32], FundingAccountLabelTooLong> {
    let prefix = match kind {
        FundingAccountKind::Deposit => "onramp:eph",
        FundingAccountKind::Refund => "onramp:rf",
        FundingAccountKind::Withdrawal => "wd:eph",
    };
    let label = format!("{prefix}:{source_id}:{number}");
    let mut index = [0u8; 32];
    index
        .get_mut(..label.len())
        .ok_or(FundingAccountLabelTooLong)?
        .copy_from_slice(label.as_bytes());
    Ok(index)
}

/// Why a session operation failed.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum FundingSessionError {
    /// The host's storage callback failed.
    #[display("funding session storage failed: {reason}")]
    Storage {
        /// Failure detail.
        reason: String,
    },
}

/// The sessions worth keeping: every open one, then the most recently settled
/// ones up to a fixed bound.
///
/// The bound exists because [`CoreStorageKey::FundingSessions`] is one SCALE
/// blob rewritten on every change; the host keeps the full history.
pub fn retained(sessions: impl IntoIterator<Item = FundingSession>) -> Vec<FundingSession> {
    let (mut open, mut settled): (Vec<_>, Vec<_>) = sessions
        .into_iter()
        .partition(|session| !session.is_terminal());
    settled.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
    settled.truncate(SETTLED_HISTORY_LIMIT);
    open.append(&mut settled);
    open
}

/// Read every persisted session.
///
/// A blob that does not decode is discarded with a warning rather than
/// returned as an error, so one bad write cannot disable funding for good.
pub async fn load_sessions(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
    let Some(blob) = storage
        .read_core_storage(CoreStorageKey::FundingSessions)
        .await
        .map_err(|err| FundingSessionError::Storage { reason: err.reason })?
    else {
        return Ok(Vec::new());
    };
    match Vec::<FundingSession>::decode(&mut blob.as_slice()) {
        Ok(sessions) if sessions.encoded_size() == blob.len() => Ok(sessions),
        _ => {
            warn!("discarding undecodable funding sessions");
            Ok(Vec::new())
        }
    }
}

/// Replace the persisted set with `sessions`, clearing the slot when empty.
pub async fn store_sessions(
    storage: &(impl CoreStorage + ?Sized),
    sessions: &[FundingSession],
) -> Result<(), FundingSessionError> {
    let written = if sessions.is_empty() {
        storage
            .clear_core_storage(CoreStorageKey::FundingSessions)
            .await
    } else {
        storage
            .write_core_storage(CoreStorageKey::FundingSessions, sessions.encode())
            .await
    };
    written.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

/// Reserve the next account number for `source_id`, counting up from 1.
///
/// Counters are never reset, so no two sessions on this device share an
/// account. A blob that does not decode is an error rather than a reset for
/// the same reason. Counters are per device: the same seed on a new install
/// starts from 1 again, so whoever hands an account to a provider must first
/// check it is empty on chain.
pub async fn next_account_number(
    storage: &(impl CoreStorage + ?Sized),
    source_id: &str,
) -> Result<u32, FundingSessionError> {
    let storage_error = |reason: String| FundingSessionError::Storage { reason };
    let mut counters = match storage
        .read_core_storage(CoreStorageKey::FundingAccountCounters)
        .await
        .map_err(|err| storage_error(err.reason))?
    {
        Some(blob) => BTreeMap::<String, u32>::decode(&mut blob.as_slice())
            .ok()
            .filter(|counters| counters.encoded_size() == blob.len())
            .ok_or_else(|| storage_error("funding account counters do not decode".into()))?,
        None => BTreeMap::new(),
    };
    let counter = counters.entry(source_id.to_string()).or_default();
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| storage_error(format!("funding accounts for {source_id} exhausted")))?;
    let number = *counter;
    storage
        .write_core_storage(CoreStorageKey::FundingAccountCounters, counters.encode())
        .await
        .map_err(|err| storage_error(err.reason))?;
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::block_on;

    use crate::test_support::stub_platform;

    const NOW: u64 = 1_700_000_000_000;

    fn session(direction: FundingDirection) -> FundingSession {
        FundingSession::new(
            "fs_1".to_string(),
            Some("wallet.dot".to_string()),
            direction,
            Some(100),
            NOW,
        )
    }

    fn expired(intent: &str, settled_at_ms: u64) -> FundingSession {
        FundingSession {
            intent: intent.to_string(),
            stage: FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms,
            },
            ..session(FundingDirection::In)
        }
    }

    #[test]
    fn an_open_session_shows_the_screen_for_its_direction() {
        assert_eq!(
            (
                session(FundingDirection::In).wire_item(),
                session(FundingDirection::Out).wire_item(),
            ),
            (
                HostFundingStatusSubscribeItem::AwaitingDeposit {
                    expires_at: Some(NOW + SESSION_WINDOW_MS),
                },
                HostFundingStatusSubscribeItem::AwaitingRelease,
            )
        );
    }

    // Nothing else ends a session that hears nothing, so expiry is what
    // guarantees every subscriber eventually gets a terminal item.
    #[test]
    fn a_session_expires_at_its_deadline_and_not_before() {
        let mut session = session(FundingDirection::In);

        assert!(!session.expire_if_due(NOW + SESSION_WINDOW_MS - 1));
        assert!(session.expire_if_due(NOW + SESSION_WINDOW_MS));
        assert_eq!(
            session.wire_item(),
            HostFundingStatusSubscribeItem::Failed {
                reason: FundingFailure::Expired,
                moved: 0,
            }
        );
    }

    #[test]
    fn an_ended_session_keeps_its_first_outcome() {
        let mut session = expired("fs_1", NOW);

        assert!(!session.fail(FundingFailure::Cancelled, NOW + 1));
        assert_eq!(session, expired("fs_1", NOW));
    }

    #[test]
    fn open_sessions_come_first_and_settled_history_keeps_the_newest() {
        let open = session(FundingDirection::Out);
        let settled = (0..SETTLED_HISTORY_LIMIT + 1)
            .map(|index| expired(&format!("fs_s{index}"), NOW + index as u64));

        let kept: Vec<String> = retained(settled.chain([open]))
            .into_iter()
            .map(|session| session.intent)
            .collect();

        let newest_first = (1..=SETTLED_HISTORY_LIMIT).rev().map(|index| format!("fs_s{index}"));
        assert_eq!(
            kept,
            std::iter::once("fs_1".to_string())
                .chain(newest_first)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sessions_round_trip_through_core_storage() {
        let storage = stub_platform();
        let sessions = vec![session(FundingDirection::In), expired("fs_2", NOW)];

        block_on(store_sessions(storage.as_ref(), &sessions)).expect("stored");

        assert_eq!(
            block_on(load_sessions(storage.as_ref())).expect("loaded"),
            sessions
        );
    }

    #[test]
    fn an_undecodable_blob_reads_as_no_sessions() {
        let storage = stub_platform();
        let mut blob = vec![session(FundingDirection::In)].encode();
        blob.push(0xff);
        block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob))
            .expect("written");

        assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
    }

    // A reused number hands a second session an account that may still hold
    // the first one's funds, so counters only ever move up, per source, and a
    // counter blob that cannot be read stops funding instead of restarting.
    #[test]
    fn account_numbers_count_up_per_source_and_never_restart() {
        let storage = stub_platform();
        let next = |source: &str| block_on(next_account_number(storage.as_ref(), source));
        let issued = [next("usdt"), next("usdt"), next("btc"), next("usdt")];
        block_on(storage.write_core_storage(CoreStorageKey::FundingAccountCounters, vec![0xff]))
            .expect("written");

        assert_eq!(
            (issued, next("usdt").is_err()),
            ([Ok(1), Ok(2), Ok(1), Ok(3)], true)
        );
    }

    const SUBMISSION: ConversionSubmission = ConversionSubmission {
        nonce: 4,
        submitted_at_ms: NOW,
        valid_until_block: 164,
        people_before: 0,
        landing: 40,
        spent: 50,
    };

    fn converting() -> FundingSession {
        FundingSession {
            stage: FundingStage::Converting {
                deposited: 50,
                refusals: 0,
                submission: None,
            },
            deposit: Some(FundingDeposit {
                source_id: "usdt-assethub".to_string(),
                number: 1,
                asset: DepositAsset::Asset(1984),
                account: [1; 32],
                expected: 50,
                route: ConversionRoute::Teleport,
            }),
            ..session(FundingDirection::In)
        }
    }

    // A dry run that keeps refusing will not start passing, so the third
    // refusal ends the session rather than retrying forever, and a refusal
    // clears any submission so the next attempt starts clean.
    #[test]
    fn a_conversion_ends_on_its_third_refusal() {
        let mut session = converting();
        session.advance_conversion(ConversionStep::Submitted(SUBMISSION), NOW);
        let refused = |session: &mut FundingSession| {
            session.advance_conversion(
                ConversionStep::Refused {
                    reason: "no pool".into(),
                },
                NOW,
            );
            session.stage.clone()
        };

        assert_eq!(
            [refused(&mut session), refused(&mut session), refused(&mut session)],
            [
                FundingStage::Converting {
                    deposited: 50,
                    refusals: 1,
                    submission: None,
                },
                FundingStage::Converting {
                    deposited: 50,
                    refusals: 2,
                    submission: None,
                },
                FundingStage::Failed {
                    reason: FundingFailure::Other {
                        code: "conversion_refused".into(),
                        message: "no pool".into(),
                    },
                    settled_at_ms: NOW,
                },
            ]
        );
    }

    // CASH on People is what the user is owed, so landing ends conversion
    // whatever the submission state, and the subscriber keeps seeing
    // converting until it is credited.
    #[test]
    fn landing_on_people_ends_the_conversion() {
        let mut session = converting();
        session.advance_conversion(ConversionStep::Submitted(SUBMISSION), NOW);
        let submitted = session.converting().and_then(|(_, submission)| submission);
        session.advance_conversion(ConversionStep::Landed { landed: 49 }, NOW);

        assert_eq!(
            (submitted, session.stage.clone(), session.wire_item(), session.is_terminal()),
            (
                Some(SUBMISSION),
                FundingStage::Converted { landed: 49 },
                HostFundingStatusSubscribeItem::Converting,
                false,
            )
        );
    }

    // Funds sit in these accounts, so an index that drifts between releases
    // strands them. The bytes are pinned to the labels getcash uses.
    #[test]
    fn funding_account_indices_are_the_padded_getcash_labels() {
        let padded = |label: &str| {
            let mut index = [0u8; 32];
            index[..label.len()].copy_from_slice(label.as_bytes());
            index
        };

        assert_eq!(
            [
                funding_account_index(FundingAccountKind::Deposit, "usdt-assethub", 1),
                funding_account_index(FundingAccountKind::Refund, "btc", 2),
                funding_account_index(FundingAccountKind::Withdrawal, "dot-assethub", 3),
                funding_account_index(FundingAccountKind::Deposit, "x".repeat(40).as_str(), 1),
            ],
            [
                Ok(padded("onramp:eph:usdt-assethub:1")),
                Ok(padded("onramp:rf:btc:2")),
                Ok(padded("wd:eph:dot-assethub:3")),
                Err(FundingAccountLabelTooLong),
            ]
        );
    }

    #[test]
    fn storing_nothing_clears_the_slot() {
        let storage = stub_platform();
        block_on(store_sessions(
            storage.as_ref(),
            &[session(FundingDirection::In)],
        ))
        .expect("stored");

        block_on(store_sessions(storage.as_ref(), &[])).expect("cleared");

        assert_eq!(
            block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)),
            Ok(None)
        );
    }
}
