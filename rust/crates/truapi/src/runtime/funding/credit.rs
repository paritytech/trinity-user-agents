//! Crediting landed CASH into the user's balance, the way getcash does it.
//!
//! The host's top-up claims the CASH on the deposit account on People, given
//! the account's secret key. Each attempt has its own id, so a retried call
//! for the same attempt is answered `AlreadyExists` and never claims twice,
//! and each is sized from what the account holds when it starts. A claim
//! that falls short moves on to the next attempt; after the last, crediting
//! settles for what was claimed, or fails with the CASH still on the account
//! when nothing was, for a retry or the same key to claim later.

use core::time::Duration;

use futures::StreamExt;
use truapi::latest::{
    GenericError, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpStatusSubscribeError, HostPaymentTopUpStatusSubscribeItem, PaymentTopUpSource,
};

use super::FundingSigner;
use crate::host_logic::funding::{CreditProgress, CreditStep, FundingDeposit, funding_attempt_id};
use crate::platform::{ProductContext, TopUpPlatform};

/// Smallest amount a top-up claims, in CASH units: what the account holds is
/// claimed rounded down to it.
pub const CLAIM_UNIT: u128 = 10_000;
/// How long one registered top-up may run before crediting times out.
const ATTEMPT_WINDOW_MS: u64 = 90 * 60 * 1_000;
/// How long crediting may take in all before it times out, so a claim that
/// never finalizes or a host that keeps answering busy cannot hold a session
/// open for good.
const CREDIT_DEADLINE_MS: u64 = 4 * ATTEMPT_WINDOW_MS;
/// Longest the host may take to report a top-up's current status.
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);

/// What crediting one session needs.
pub struct Credit<'a> {
    /// The host's top-up.
    pub top_up: &'a dyn TopUpPlatform,
    /// Holds the deposit account's key.
    pub signer: &'a dyn FundingSigner,
    /// The funding product the top-ups are made as.
    pub product: &'a ProductContext,
}

impl Credit<'_> {
    /// Decide the next step for a session crediting `deposit`'s CASH, given
    /// its `progress` so far. `held` is the account's CASH on People, read
    /// when the next top-up has yet to be sized.
    ///
    /// A sized top-up is kept before it is registered, so one registered
    /// just before a restart is registered again for the same amount under
    /// the same id rather than sized from what is left. What the host
    /// reports is applied before any time limit, so a claim that finished
    /// late still counts.
    pub async fn plan(
        &self,
        deposit: &FundingDeposit,
        progress: Option<CreditProgress>,
        held: Option<u128>,
        now_ms: u64,
    ) -> Result<Option<CreditStep>, GenericError> {
        let attempt = progress.map_or(0, |progress| progress.attempt);
        let past_deadline =
            progress.is_some_and(|progress| now_ms.saturating_sub(progress.started_ms) > CREDIT_DEADLINE_MS);
        let claim = match progress.and_then(|progress| progress.claim) {
            None if past_deadline => return Ok(Some(CreditStep::TimedOut)),
            None => {
                let Some(held) = held else {
                    return Ok(None);
                };
                let amount = held - held % CLAIM_UNIT;
                return Ok(Some(if amount == 0 {
                    CreditStep::Drained
                } else {
                    CreditStep::Sized { amount }
                }));
            }
            Some(claim) if !claim.registered && past_deadline => return Ok(Some(CreditStep::TimedOut)),
            Some(claim) if !claim.registered => return self.register(deposit, attempt, claim.amount).await,
            Some(claim) => claim,
        };
        let status = self.status(deposit, attempt).await;
        let terminal = match status {
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true })) => Some(CreditStep::Claimed {
                claimed: claim.amount,
            }),
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed })) => {
                Some(CreditStep::Short {
                    claimed: actual_claimed,
                })
            }
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed)) => Some(CreditStep::Short { claimed: 0 }),
            _ => None,
        };
        if terminal.is_some() {
            return Ok(terminal);
        }
        // As getcash, an attempt the host has not finished in its window
        // times out whatever it last reported.
        if past_deadline || now_ms.saturating_sub(claim.since_ms) > ATTEMPT_WINDOW_MS {
            return Ok(Some(CreditStep::TimedOut));
        }
        match status {
            // Registered again under the same id; recording that changes
            // nothing, so the attempt's window keeps running.
            Some(Err(HostPaymentTopUpStatusSubscribeError::NotFound)) => {
                self.register(deposit, attempt, claim.amount).await
            }
            _ => Ok(None),
        }
    }

    /// Ask the host to claim `amount` from the deposit account as `attempt`.
    async fn register(
        &self,
        deposit: &FundingDeposit,
        attempt: u8,
        amount: u128,
    ) -> Result<Option<CreditStep>, GenericError> {
        let keypair = self
            .signer
            .deposit_keypair(&deposit.source_id, deposit.number)?
            .filter(|keypair| keypair.public.to_bytes() == deposit.account);
        let Some(keypair) = keypair else {
            return Ok(None);
        };
        // Canonical schnorrkel bytes, the form getcash converts its burner
        // key to before handing it to the host's top-up.
        let request = HostPaymentTopUpRequest {
            into: None,
            amount,
            source: PaymentTopUpSource::PrivateKey {
                sr25519_secret_key: keypair.secret.to_bytes(),
            },
            id: funding_attempt_id(&deposit.account, attempt),
        };
        match self.top_up.top_up(self.product, request).await {
            Ok(()) | Err(HostPaymentTopUpError::AlreadyExists) => {
                Ok(Some(CreditStep::Registered))
            }
            Err(HostPaymentTopUpError::InvalidSource) => Ok(Some(CreditStep::Refused {
                reason: "the host refused the deposit account as a top-up source".into(),
            })),
            Err(HostPaymentTopUpError::SourceBusy | HostPaymentTopUpError::Unknown { .. }) => {
                Ok(None)
            }
        }
    }

    /// The current status of `attempt`, or `None` if the host reports none
    /// in time.
    async fn status(
        &self,
        deposit: &FundingDeposit,
        attempt: u8,
    ) -> Option<Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>>
    {
        let mut statuses = self
            .top_up
            .subscribe_top_up_status(self.product, funding_attempt_id(&deposit.account, attempt));
        super::within_timeout(STATUS_TIMEOUT, statuses.next())
            .await
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::executor::block_on;
    use futures::stream::{self, BoxStream};

    use super::*;
    use crate::host_logic::funding::{Claim, ConversionRoute, DepositAsset};
    use crate::platform::async_trait;

    const NOW: u64 = 1_700_000_000_000;

    /// A top-up that answers fixed results and records every request.
    struct Host {
        accepts: Result<(), HostPaymentTopUpError>,
        status: Option<Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>>,
        requests: Mutex<Vec<HostPaymentTopUpRequest>>,
    }

    impl Host {
        fn new(
            accepts: Result<(), HostPaymentTopUpError>,
            status: Option<Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>>,
        ) -> Self {
            Self {
                accepts,
                status,
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<(u128, [u8; 32])> {
            self.requests
                .lock()
                .expect("requests")
                .iter()
                .map(|request| (request.amount, request.id))
                .collect()
        }
    }

    #[async_trait]
    impl TopUpPlatform for Host {
        async fn top_up(
            &self,
            _product: &ProductContext,
            request: HostPaymentTopUpRequest,
        ) -> Result<(), HostPaymentTopUpError> {
            self.requests.lock().expect("requests").push(request);
            self.accepts.clone()
        }

        fn subscribe_top_up_status(
            &self,
            _product: &ProductContext,
            _id: [u8; 32],
        ) -> BoxStream<
            'static,
            Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>,
        > {
            stream::iter(self.status.clone()).boxed()
        }
    }

    struct Keys(schnorrkel::Keypair);

    impl FundingSigner for Keys {
        fn deposit_keypair(
            &self,
            _: &str,
            _: u32,
        ) -> Result<Option<schnorrkel::Keypair>, GenericError> {
            Ok(Some(self.0.clone()))
        }

        fn withdrawal_keypair(
            &self,
            _: &str,
            _: u32,
        ) -> Result<Option<schnorrkel::Keypair>, GenericError> {
            Ok(Some(self.0.clone()))
        }

        fn funding_product_id(&self) -> String {
            "fund.dot".into()
        }
    }

    fn keypair(seed: u8) -> schnorrkel::Keypair {
        schnorrkel::MiniSecretKey::from_bytes(&[seed; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
    }

    fn deposit() -> FundingDeposit {
        FundingDeposit {
            source_id: "usdt-assethub".into(),
            number: 1,
            asset: DepositAsset::Asset(1984),
            account: keypair(1).public.to_bytes(),
            expected: 2_000_000,
            route: ConversionRoute::Psm { fee_ppm: 5_000 },
            target: None,
            holdings: Vec::new(),
        }
    }

    fn plan(
        host: &Host,
        key: u8,
        progress: Option<CreditProgress>,
        held: Option<u128>,
        now_ms: u64,
    ) -> Option<CreditStep> {
        let product = ProductContext {
            product_id: "fund.dot".into(),
            execution_kind: Default::default(),
        };
        let keys = Keys(keypair(key));
        let credit = Credit {
            top_up: host,
            signer: &keys,
            product: &product,
        };
        block_on(credit.plan(&deposit(), progress, held, now_ms)).expect("planned")
    }

    fn claim(attempt: u8, amount: u128, registered: bool) -> Option<CreditProgress> {
        Some(CreditProgress {
            credited: 0,
            attempt,
            claim: Some(Claim {
                amount,
                since_ms: NOW,
                registered,
            }),
            started_ms: NOW,
        })
    }

    // The first top-up is the one getcash would make: what the account holds
    // rounded down to the claim unit, kept before it is registered, then
    // registered under the account's id, so a top-up the host already holds
    // is not made twice.
    #[test]
    fn landed_cash_is_sized_then_claimed_once_under_the_accounts_id() {
        let fresh = Host::new(Ok(()), None);
        let known = Host::new(Err(HostPaymentTopUpError::AlreadyExists), None);
        let account = deposit().account;
        let sized = claim(0, 1_980_000, false);

        assert_eq!(
            (
                plan(&fresh, 1, None, Some(1_987_654), NOW),
                plan(&fresh, 1, sized, Some(5), NOW),
                fresh.requests(),
                plan(&known, 1, sized, None, NOW),
            ),
            (
                Some(CreditStep::Sized { amount: 1_980_000 }),
                Some(CreditStep::Registered),
                vec![(1_980_000, account)],
                Some(CreditStep::Registered),
            )
        );
    }

    // What the host reports decides the step, even past a time limit, so a
    // claim that finished late still counts: a final claim of everything, a
    // short one whose remainder the next attempt claims. An attempt the host
    // has not finished in its window times out rather than starting another
    // top-up that could claim alongside it.
    #[test]
    fn each_top_up_status_is_settled_as_getcash_settles_it() {
        let status = |item| Host::new(Ok(()), Some(Ok(item)));
        let finalized = status(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true });
        let unfinalized = status(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false });
        let detecting = status(HostPaymentTopUpStatusSubscribeItem::Detecting);
        let overdue = NOW + ATTEMPT_WINDOW_MS + 1;
        let running = claim(0, 1_980_000, true);

        assert_eq!(
            [
                plan(&finalized, 1, running, None, NOW + CREDIT_DEADLINE_MS + 1),
                plan(
                    &status(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially {
                        actual_claimed: 1_000_000,
                    }),
                    1,
                    running,
                    None,
                    NOW,
                ),
                plan(&status(HostPaymentTopUpStatusSubscribeItem::NotClaimed), 1, running, None, NOW),
                plan(&detecting, 1, running, None, NOW),
                plan(&detecting, 1, running, None, overdue),
                plan(&unfinalized, 1, running, None, NOW),
                plan(&unfinalized, 1, running, None, overdue),
            ],
            [
                Some(CreditStep::Claimed { claimed: 1_980_000 }),
                Some(CreditStep::Short { claimed: 1_000_000 }),
                Some(CreditStep::Short { claimed: 0 }),
                None,
                Some(CreditStep::TimedOut),
                None,
                Some(CreditStep::TimedOut),
            ]
        );
    }

    // A top-up the host lost is registered again with the same amount under
    // the same id, never re-sized from what is left after it may have
    // claimed part.
    #[test]
    fn a_lost_top_up_is_registered_again_as_it_was() {
        let lost = Host::new(Ok(()), Some(Err(HostPaymentTopUpStatusSubscribeError::NotFound)));

        assert_eq!(
            (plan(&lost, 1, claim(0, 1_980_000, true), Some(10_000), NOW), lost.requests()),
            (Some(CreditStep::Registered), vec![(1_980_000, deposit().account)])
        );
    }

    // A later attempt claims what is left on the account, read again, under
    // its own id; with less left than a top-up claims there is nothing more
    // to try, and without a reading nothing is sized.
    #[test]
    fn a_later_attempt_claims_what_is_left_under_its_own_id() {
        let host = Host::new(Ok(()), None);
        let account = deposit().account;
        let second_id = sp_crypto_hashing::blake2_256(&[account.as_slice(), &1u32.to_le_bytes()].concat());
        let next = Some(CreditProgress {
            credited: 1_000_000,
            attempt: 1,
            claim: None,
            started_ms: NOW,
        });
        let sized = next.map(|progress| CreditProgress {
            claim: Some(Claim {
                amount: 980_000,
                since_ms: NOW,
                registered: false,
            }),
            ..progress
        });

        assert_eq!(
            (
                plan(&host, 1, next, Some(987_654), NOW),
                plan(&host, 1, next, Some(CLAIM_UNIT - 1), NOW),
                plan(&host, 1, next, None, NOW),
                plan(&host, 1, sized, None, NOW),
                host.requests(),
            ),
            (
                Some(CreditStep::Sized { amount: 980_000 }),
                Some(CreditStep::Drained),
                None,
                Some(CreditStep::Registered),
                vec![(980_000, second_id)],
            )
        );
    }

    // A session outlives a sign-out; another identity's key must not be
    // handed to the host as this account's.
    #[test]
    fn only_the_deposit_accounts_key_is_handed_to_the_host() {
        let host = Host::new(Ok(()), None);

        assert_eq!(
            (plan(&host, 2, claim(0, 1_980_000, false), None, NOW), host.requests()),
            (None, Vec::new())
        );
    }
}
