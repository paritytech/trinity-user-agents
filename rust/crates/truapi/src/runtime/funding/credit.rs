//! Crediting landed CASH into the user's balance, the way getcash does it.
//!
//! The host's top-up claims the CASH on the deposit account on People, given
//! the account's secret key. Each attempt has its own id, so a retried call
//! for the same attempt is answered `AlreadyExists` and never claims twice.
//! A claim that takes nothing, or never finishes, moves on to the next
//! attempt; after the last one the session fails with the CASH still on the
//! account, where the same key can claim it later.

use core::time::Duration;

use futures::StreamExt;
use truapi::latest::{
    GenericError, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpStatusSubscribeError, HostPaymentTopUpStatusSubscribeItem, PaymentTopUpSource,
};

use super::FundingSigner;
use crate::host_logic::funding::{CreditAttempt, CreditStep, FundingDeposit};
use crate::platform::{ProductContext, TopUpPlatform};

/// Smallest amount a top-up claims, in CASH units: the landed CASH is
/// claimed rounded down to it.
const CLAIM_UNIT: u128 = 10_000;
/// Top-up attempts before crediting gives up.
const MAX_ATTEMPTS: u8 = 3;
/// How long one attempt may run before the next replaces it.
const ATTEMPT_WINDOW_MS: u64 = 90 * 60 * 1_000;
/// How long crediting may take in all before it gives up, so a claim that
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
    /// Decide the next step for a session whose CASH `landed` on `deposit`'s
    /// account, given the attempt running and when it started.
    pub async fn plan(
        &self,
        deposit: &FundingDeposit,
        landed: u128,
        running: Option<CreditAttempt>,
        now_ms: u64,
    ) -> Result<Option<CreditStep>, GenericError> {
        let amount = landed - landed % CLAIM_UNIT;
        if amount == 0 {
            return Ok(Some(CreditStep::Abandoned {
                reason: "less CASH landed than a top-up can claim".into(),
            }));
        }
        let Some(CreditAttempt {
            attempt,
            since_ms,
            started_ms,
        }) = running
        else {
            return self.register(deposit, amount, 0).await;
        };
        if now_ms.saturating_sub(started_ms) > CREDIT_DEADLINE_MS {
            return Ok(Some(CreditStep::Abandoned {
                reason: "crediting did not finish in time".into(),
            }));
        }
        let status = self.status(deposit, attempt).await;
        let overdue = now_ms.saturating_sub(since_ms) > ATTEMPT_WINDOW_MS;
        match status {
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true })) => {
                Ok(Some(CreditStep::Credited { credited: amount }))
            }
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed })) => {
                Ok(Some(CreditStep::Credited {
                    credited: actual_claimed,
                }))
            }
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed)) => {
                self.next_attempt(deposit, amount, attempt).await
            }
            Some(Ok(
                HostPaymentTopUpStatusSubscribeItem::Detecting
                | HostPaymentTopUpStatusSubscribeItem::Claiming,
            )) if overdue => self.next_attempt(deposit, amount, attempt).await,
            Some(Err(HostPaymentTopUpStatusSubscribeError::NotFound)) => {
                self.register(deposit, amount, attempt).await
            }
            Some(Ok(_)) | Some(Err(HostPaymentTopUpStatusSubscribeError::Unknown { .. })) | None => {
                Ok(None)
            }
        }
    }

    async fn next_attempt(
        &self,
        deposit: &FundingDeposit,
        amount: u128,
        attempt: u8,
    ) -> Result<Option<CreditStep>, GenericError> {
        let next = attempt + 1;
        if next >= MAX_ATTEMPTS {
            return Ok(Some(CreditStep::Abandoned {
                reason: "no top-up claimed the CASH".into(),
            }));
        }
        self.register(deposit, amount, next).await
    }

    /// Ask the host to claim `amount` from the deposit account as `attempt`.
    async fn register(
        &self,
        deposit: &FundingDeposit,
        amount: u128,
        attempt: u8,
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
            id: top_up_id(&deposit.account, attempt),
        };
        match self.top_up.top_up(self.product, request).await {
            Ok(()) | Err(HostPaymentTopUpError::AlreadyExists) => {
                Ok(Some(CreditStep::Registered { attempt }))
            }
            Err(HostPaymentTopUpError::InvalidSource) => Ok(Some(CreditStep::Abandoned {
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
            .subscribe_top_up_status(self.product, top_up_id(&deposit.account, attempt));
        super::within_timeout(STATUS_TIMEOUT, statuses.next())
            .await
            .ok()
            .flatten()
    }
}

/// The id of top-up `attempt` from `account`: the account itself first, then
/// `blake2_256(account ‖ attempt)`, as getcash numbers them.
fn top_up_id(account: &[u8; 32], attempt: u8) -> [u8; 32] {
    if attempt == 0 {
        return *account;
    }
    sp_crypto_hashing::blake2_256(&[account.as_slice(), &u32::from(attempt).to_le_bytes()].concat())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::executor::block_on;
    use futures::stream::{self, BoxStream};

    use super::*;
    use crate::host_logic::funding::{ConversionRoute, DepositAsset};
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
        }
    }

    fn plan(
        host: &Host,
        key: u8,
        running: Option<CreditAttempt>,
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
        block_on(credit.plan(&deposit(), 1_987_654, running, now_ms)).expect("planned")
    }

    // The first top-up is the one getcash would make: the landed CASH rounded
    // down to the claim unit, identified by the account, so a top-up the host
    // already holds is not made twice.
    #[test]
    fn landed_cash_is_claimed_once_under_the_accounts_id() {
        let fresh = Host::new(Ok(()), None);
        let known = Host::new(Err(HostPaymentTopUpError::AlreadyExists), None);
        let account = deposit().account;

        assert_eq!(
            (
                plan(&fresh, 1, None, NOW),
                fresh.requests(),
                plan(&known, 1, None, NOW),
            ),
            (
                Some(CreditStep::Registered { attempt: 0 }),
                vec![(1_980_000, account)],
                Some(CreditStep::Registered { attempt: 0 }),
            )
        );
    }

    // Only a finalized claim credits; one that is claimed but unfinalized is
    // waited for rather than retried, since a fresh attempt would find
    // nothing to claim, until crediting as a whole runs out of time.
    #[test]
    fn only_a_finalized_claim_credits_the_balance() {
        let overdue = NOW + ATTEMPT_WINDOW_MS + 1;
        let unfinalized = Host::new(
            Ok(()),
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false })),
        );
        let finalized = Host::new(
            Ok(()),
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true })),
        );
        let partial = Host::new(
            Ok(()),
            Some(Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially {
                actual_claimed: 1_000_000,
            })),
        );

        assert_eq!(
            [
                plan(&unfinalized, 1, Some(CreditAttempt { attempt: 0, since_ms: NOW, started_ms: NOW }), overdue),
                plan(
                    &unfinalized,
                    1,
                    Some(CreditAttempt { attempt: 0, since_ms: NOW, started_ms: NOW }),
                    NOW + CREDIT_DEADLINE_MS + 1,
                ),
                plan(&finalized, 1, Some(CreditAttempt { attempt: 0, since_ms: NOW, started_ms: NOW }), NOW),
                plan(&partial, 1, Some(CreditAttempt { attempt: 0, since_ms: NOW, started_ms: NOW }), NOW),
            ],
            [
                None,
                Some(CreditStep::Abandoned {
                    reason: "crediting did not finish in time".into(),
                }),
                Some(CreditStep::Credited {
                    credited: 1_980_000
                }),
                Some(CreditStep::Credited {
                    credited: 1_000_000
                }),
            ]
        );
    }

    // A claim that took nothing, or is stuck, gets a fresh attempt under a
    // new id, up to the last, after which the CASH stays on the account.
    #[test]
    fn an_unclaimed_top_up_is_retried_under_a_new_id_until_the_last() {
        let not_claimed = Host::new(Ok(()), Some(Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed)));
        let stuck = Host::new(Ok(()), Some(Ok(HostPaymentTopUpStatusSubscribeItem::Detecting)));
        let account = deposit().account;
        let second_id = sp_crypto_hashing::blake2_256(&[account.as_slice(), &1u32.to_le_bytes()].concat());

        assert_eq!(
            (
                plan(&not_claimed, 1, Some(CreditAttempt { attempt: 0, since_ms: NOW, started_ms: NOW }), NOW),
                not_claimed.requests(),
                plan(&stuck, 1, Some(CreditAttempt { attempt: 1, since_ms: NOW, started_ms: NOW }), NOW),
                plan(&stuck, 1, Some(CreditAttempt { attempt: 1, since_ms: NOW, started_ms: NOW }), NOW + ATTEMPT_WINDOW_MS + 1),
                plan(&not_claimed, 1, Some(CreditAttempt { attempt: 2, since_ms: NOW, started_ms: NOW }), NOW),
            ),
            (
                Some(CreditStep::Registered { attempt: 1 }),
                vec![(1_980_000, second_id)],
                None,
                Some(CreditStep::Registered { attempt: 2 }),
                Some(CreditStep::Abandoned {
                    reason: "no top-up claimed the CASH".into(),
                }),
            )
        );
    }

    // A session outlives a sign-out; another identity's key must not be
    // handed to the host as this account's.
    #[test]
    fn only_the_deposit_accounts_key_is_handed_to_the_host() {
        let host = Host::new(Ok(()), None);

        assert_eq!((plan(&host, 2, None, NOW), host.requests()), (None, Vec::new()));
    }
}
