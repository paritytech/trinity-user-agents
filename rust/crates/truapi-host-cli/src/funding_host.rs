//! Scripted funding host for the CLI test host.
//!
//! It stands in for the host's funding overlay and the provider behind it:
//! each request a product makes is answered with the next outcome from
//! `TRUAPI_FUNDING_OUTCOMES` (`deliver:<credited>`, `release:<debited>`,
//! `fail` or `dismiss`, comma-separated), and a session the user started is
//! then settled through the core's test hook, as though its funds had moved.
//! There is no chain behind it: this exists so a battery can drive the
//! product's side of funding headlessly.
//!
//! `provide` and `provide-cancel` instead hand the session to the product that
//! asked, so a provider worker can serve its own request; `provide-cancel`
//! then cancels it, as a user would. Top-ups and payment requests that worker
//! starts are accepted and complete in full, recorded in the ledger named by
//! `TRUAPI_FUNDING_LEDGER` so their statuses outlive a restart.
//!
//! Every overlay request and every session change the core reports is
//! appended to the transcript named by `TRUAPI_FUNDING_LOG`, one JSON object
//! per line, so a battery can assert what the host saw rather than only what
//! the product was told.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use futures::stream::{self, BoxStream, StreamExt};
use truapi::SigningHostRuntime;
use truapi::host_logic::funding::FundingStage;
use truapi::latest::{
    FundingDirection, FundingFailure, FundingFrameOutcome, GenericError,
    HostFundingStatusSubscribeItem, HostPaymentError, HostPaymentRequest,
    HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
    HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
    HostPaymentTopUpStatusSubscribeItem,
};
use truapi::platform::{
    FundingPlatform, FundingPresentOutcome, FundingPresentation, PaymentPlatform, ProductContext,
    TopUpPlatform, async_trait,
};

/// How long a started session stays in flight before it is settled, so a
/// product sees its first status before the terminal one.
const SETTLE_AFTER: Duration = Duration::from_millis(500);

/// What the scripted provider does with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The user starts and the funds arrive.
    Deliver(u128),
    /// The user starts and the funds leave.
    Release(u128),
    /// The user starts and the provider fails.
    Fail,
    /// The user closes the overlay.
    Dismiss,
    /// The user picks the product that asked as the provider.
    Provide,
    /// As `Provide`, then the user cancels.
    ProvideThenCancel,
}

/// Parse `deliver:900,release:500,fail,dismiss`.
fn parse_outcomes(spec: &str) -> VecDeque<Outcome> {
    spec.split(',')
        .map(str::trim)
        .filter_map(|entry| match entry.split_once(':') {
            Some(("deliver", amount)) => amount.parse().ok().map(Outcome::Deliver),
            Some(("release", amount)) => amount.parse().ok().map(Outcome::Release),
            None if entry == "fail" => Some(Outcome::Fail),
            None if entry == "dismiss" => Some(Outcome::Dismiss),
            None if entry == "provide" => Some(Outcome::Provide),
            None if entry == "provide-cancel" => Some(Outcome::ProvideThenCancel),
            _ => None,
        })
        .collect()
}

/// A funding overlay and provider that follow a script.
pub struct CliFundingHost {
    outcomes: Mutex<VecDeque<Outcome>>,
    transcript: Option<PathBuf>,
    runtime: OnceLock<Weak<SigningHostRuntime>>,
}

impl CliFundingHost {
    /// Install a scripted funding host on `runtime` when
    /// `TRUAPI_FUNDING_OUTCOMES` is set, recording to `TRUAPI_FUNDING_LOG`.
    pub fn install_from_env(runtime: &Arc<SigningHostRuntime>) {
        let Ok(spec) = std::env::var("TRUAPI_FUNDING_OUTCOMES") else {
            return;
        };
        let transcript = std::env::var_os("TRUAPI_FUNDING_LOG").map(PathBuf::from);
        // Truncated at startup, so a run never reads an earlier run's lines.
        if let Some(path) = transcript.as_ref()
            && let Err(error) = std::fs::write(path, b"")
        {
            tracing::warn!(?path, %error, "funding transcript could not be truncated");
        }
        let host = Arc::new(Self {
            outcomes: Mutex::new(parse_outcomes(&spec)),
            transcript,
            runtime: OnceLock::new(),
        });
        let _ = host.runtime.set(Arc::downgrade(runtime));
        runtime.set_funding_platform(host);
        if let Some(ledger) = std::env::var_os("TRUAPI_FUNDING_LEDGER").map(PathBuf::from) {
            let payments = Arc::new(CliPaymentHost { ledger });
            runtime.set_top_up_platform(payments.clone());
            runtime.set_payment_platform(payments);
        }
    }

    /// Append one line to the transcript, in a single write so concurrent
    /// lines never interleave.
    fn record(&self, line: serde_json::Value) {
        let Some(path) = self.transcript.as_ref() else {
            return;
        };
        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
        if let Err(error) = appended {
            tracing::warn!(?path, %error, "funding transcript could not be appended to");
        }
    }

    /// Hand session `intent` to the product that asked for it, and cancel it
    /// shortly after when `cancel` is set.
    async fn provide(
        &self,
        runtime: Option<Weak<SigningHostRuntime>>,
        product: Option<&ProductContext>,
        intent: String,
        cancel: bool,
    ) -> Result<FundingPresentOutcome, GenericError> {
        let (Some(runtime), Some(product)) =
            (runtime.and_then(|runtime| runtime.upgrade()), product)
        else {
            return Ok(FundingPresentOutcome::Dismissed);
        };
        runtime
            .select_funding_provider(&intent, &product.product_id)
            .await?;
        if cancel {
            let runtime = Arc::downgrade(&runtime);
            tokio::spawn(async move {
                tokio::time::sleep(SETTLE_AFTER).await;
                let Some(runtime) = runtime.upgrade() else {
                    return;
                };
                if let Err(error) = runtime.cancel_funding(&intent).await {
                    tracing::warn!(%intent, reason = %error.reason, "cancelling a scripted funding session failed");
                }
            });
        }
        Ok(FundingPresentOutcome::Started)
    }

    fn next_outcome(&self) -> Outcome {
        self.outcomes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .unwrap_or(Outcome::Dismiss)
    }
}

/// The stage `outcome` ends a started session in.
fn settled_stage(outcome: Outcome) -> Option<FundingStage> {
    let settled_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        });
    match outcome {
        Outcome::Deliver(credited) => Some(FundingStage::Delivered {
            credited,
            settled_at_ms,
        }),
        Outcome::Release(debited) => Some(FundingStage::Released {
            debited,
            settled_at_ms,
        }),
        Outcome::Fail => Some(FundingStage::Failed {
            reason: FundingFailure::Other {
                code: "provider_failed".into(),
                message: "the scripted provider failed the session".into(),
            },
            settled_at_ms,
        }),
        Outcome::Dismiss | Outcome::Provide | Outcome::ProvideThenCancel => None,
    }
}

#[async_trait]
impl FundingPlatform for CliFundingHost {
    async fn present_funding(
        &self,
        product: Option<&ProductContext>,
        session: FundingPresentation,
    ) -> Result<FundingPresentOutcome, GenericError> {
        let outcome = self.next_outcome();
        self.record(serde_json::json!({
            "kind": "presented",
            "intent": session.intent,
            "direction": match session.direction {
                FundingDirection::In => "In",
                FundingDirection::Out => "Out",
            },
            "amount": session.amount.map(|amount| amount.to_string()),
            "product": product.map(|product| product.product_id.clone()),
            "outcome": format!("{outcome:?}"),
        }));
        let runtime = self.runtime.get().cloned();
        let intent = session.intent;
        if matches!(outcome, Outcome::Provide | Outcome::ProvideThenCancel) {
            return self
                .provide(
                    runtime,
                    product,
                    intent,
                    outcome == Outcome::ProvideThenCancel,
                )
                .await;
        }
        let Some(stage) = settled_stage(outcome) else {
            return Ok(FundingPresentOutcome::Dismissed);
        };
        tokio::spawn(async move {
            tokio::time::sleep(SETTLE_AFTER).await;
            let Some(runtime) = runtime.and_then(|runtime| runtime.upgrade()) else {
                return;
            };
            match runtime.settle_funding_for_test(&intent, stage).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(%intent, "the scripted funding session had already ended")
                }
                Err(error) => {
                    tracing::warn!(%intent, reason = %error.reason, "settling a scripted funding session failed")
                }
            }
        });
        Ok(FundingPresentOutcome::Started)
    }

    async fn present_provider_frame(
        &self,
        provider: &ProductContext,
        intent: String,
        route: String,
    ) -> Result<FundingFrameOutcome, GenericError> {
        self.record(serde_json::json!({
            "kind": "frame",
            "intent": intent,
            "provider": provider.product_id,
            "route": route,
        }));
        Ok(FundingFrameOutcome::Closed)
    }

    fn funding_session_changed(&self, intent: String, status: HostFundingStatusSubscribeItem) {
        let (tag, amount) = match status {
            HostFundingStatusSubscribeItem::AwaitingDeposit { .. } => ("AwaitingDeposit", None),
            HostFundingStatusSubscribeItem::AwaitingRelease => ("AwaitingRelease", None),
            HostFundingStatusSubscribeItem::Delivered { credited } => ("Delivered", Some(credited)),
            HostFundingStatusSubscribeItem::Released { debited } => ("Released", Some(debited)),
            HostFundingStatusSubscribeItem::Failed { .. } => ("Failed", None),
        };
        self.record(serde_json::json!({
            "kind": "status",
            "intent": intent,
            "tag": tag,
            "amount": amount.map(|amount| amount.to_string()),
        }));
    }
}

/// Scripted top-up and payment engines: every top-up claims in full and every
/// payment completes, once started. What was started is kept in a ledger file,
/// one JSON object per line, so a restarted host still answers for it.
struct CliPaymentHost {
    ledger: PathBuf,
}

impl CliPaymentHost {
    /// Whether `kind` with `id` was started by `product`.
    fn started(&self, kind: &str, product: &ProductContext, id: &[u8; 32]) -> bool {
        let id = hex::encode(id);
        std::fs::read_to_string(&self.ledger)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .any(|line| {
                line["kind"] == kind
                    && line["product"] == product.product_id.as_str()
                    && line["id"] == id.as_str()
            })
    }

    /// Record a start; `false` when the id was already used.
    fn start(&self, kind: &str, product: &ProductContext, id: &[u8; 32], amount: u128) -> bool {
        if self.started(kind, product, id) {
            return false;
        }
        let line = serde_json::json!({
            "kind": kind,
            "product": product.product_id,
            "id": hex::encode(id),
            "amount": amount.to_string(),
        });
        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.ledger)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
        if let Err(error) = appended {
            tracing::warn!(path = ?self.ledger, %error, "funding ledger could not be appended to");
        }
        true
    }
}

#[async_trait]
impl TopUpPlatform for CliPaymentHost {
    async fn top_up(
        &self,
        product: &ProductContext,
        request: HostPaymentTopUpRequest,
    ) -> Result<(), HostPaymentTopUpError> {
        if self.start("top_up", product, &request.id, request.amount) {
            Ok(())
        } else {
            Err(HostPaymentTopUpError::AlreadyExists)
        }
    }

    fn subscribe_top_up_status(
        &self,
        product: &ProductContext,
        id: [u8; 32],
    ) -> BoxStream<
        'static,
        Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>,
    > {
        if !self.started("top_up", product, &id) {
            return stream::iter([Err(HostPaymentTopUpStatusSubscribeError::NotFound)]).boxed();
        }
        stream::iter([
            Ok(HostPaymentTopUpStatusSubscribeItem::Detecting),
            Ok(HostPaymentTopUpStatusSubscribeItem::Claiming),
            Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true }),
        ])
        .boxed()
    }
}

#[async_trait]
impl PaymentPlatform for CliPaymentHost {
    async fn request_payment(
        &self,
        product: &ProductContext,
        request: HostPaymentRequest,
    ) -> Result<(), HostPaymentError> {
        if self.start("payment", product, &request.id, request.amount) {
            Ok(())
        } else {
            Err(HostPaymentError::AlreadyExists)
        }
    }

    fn subscribe_payment_status(
        &self,
        product: &ProductContext,
        id: [u8; 32],
    ) -> BoxStream<'static, Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>>
    {
        if !self.started("payment", product, &id) {
            return stream::iter([Err(HostPaymentStatusSubscribeError::PaymentNotFound)]).boxed();
        }
        stream::iter([
            Ok(HostPaymentStatusSubscribeItem::Processing),
            Ok(HostPaymentStatusSubscribeItem::Completed),
        ])
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The battery seeds one outcome per request it makes, in order; an
    // entry this host cannot read is dropped rather than shifting the rest.
    #[test]
    fn outcomes_are_read_in_order_and_unknown_entries_dropped() {
        assert_eq!(
            parse_outcomes(
                "deliver:900, release:500,fail,bogus,dismiss,deliver:x,provide,provide-cancel"
            ),
            VecDeque::from([
                Outcome::Deliver(900),
                Outcome::Release(500),
                Outcome::Fail,
                Outcome::Dismiss,
                Outcome::Provide,
                Outcome::ProvideThenCancel,
            ])
        );
    }
}
