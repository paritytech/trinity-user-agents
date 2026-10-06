//! A real on-ramp against a live network, run from the terminal.
//!
//! The command opens a funding session on a signing host, prints the deposit
//! address its provider would pay, and follows the session while someone pays
//! that address from any funded account. It ends once the session is
//! credited, or fails. The CLI has no coinage engine, so a stand-in top-up
//! checks the claim and reports it without moving coins.
//!
//! Sessions and account counters live under the state directory, so a second
//! run with `--intent` picks up the same session, and no run reuses an
//! account.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use futures::stream::{self, BoxStream, StreamExt};
use truapi::host_logic::funding::{DepositAsset, DepositRequest, FundingAccountKind, FundingStage};
use truapi::latest::{
    FundingDirection, GenericError, HostFundingStatusSubscribeItem, HostPaymentTopUpError,
    HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
    HostPaymentTopUpStatusSubscribeItem, PaymentTopUpSource,
};
use truapi::platform::{
    FundingPlatform, FundingPresentOutcome, FundingPresentation, ProductContext, TopUpPlatform,
    async_trait,
};
use truapi::{FundingNetwork, SigningHostRuntime};

use crate::network::{Network, NetworkConfig};

/// How often the command reports the session's stage.
const POLL: Duration = Duration::from_secs(6);
/// Polls a session may be missing for before the command gives up on it.
const MISSING_POLLS: u32 = 5;

/// The asset a provider pays the deposit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FundingAsset {
    /// The native token, swapped into CASH through the pools.
    Dot,
    /// dotUSD, which is CASH already: teleported to People.
    Cash,
    /// Minted into CASH through the PSM, then teleported.
    Usdt,
    /// Minted into CASH through the PSM, then teleported.
    Usdc,
}

/// Asset ids a network's Asset Hub uses for the funding assets.
struct FundingAssets {
    cash: u32,
    usdt: u32,
    usdc: u32,
}

impl FundingAssets {
    /// The ids on `network`, where they are known.
    fn of(network: Network) -> Option<Self> {
        match network {
            Network::PaseoNextV2 => Some(Self {
                cash: 50_000_413,
                usdt: 1984,
                usdc: 1337,
            }),
            Network::Previewnet => None,
        }
    }

    /// The deposit asset and the getcash source id for `asset`.
    fn source(&self, asset: FundingAsset) -> (DepositAsset, &'static str) {
        match asset {
            FundingAsset::Dot => (DepositAsset::Native, "dot-assethub"),
            FundingAsset::Cash => (DepositAsset::Asset(self.cash), "dotusd-assethub"),
            FundingAsset::Usdt => (DepositAsset::Asset(self.usdt), "usdt-assethub"),
            FundingAsset::Usdc => (DepositAsset::Asset(self.usdc), "usdc-assethub"),
        }
    }
}

/// A funding overlay that starts every session at once and prints what the
/// core reports.
struct TerminalFundingHost;

#[async_trait]
impl FundingPlatform for TerminalFundingHost {
    async fn present_funding(
        &self,
        _product: Option<&ProductContext>,
        _session: FundingPresentation,
    ) -> Result<FundingPresentOutcome, GenericError> {
        Ok(FundingPresentOutcome::Started)
    }

    fn funding_session_changed(&self, intent: String, status: HostFundingStatusSubscribeItem) {
        println!("{intent}: {status:?}");
    }
}

/// A top-up that stands in for a host's coinage engine, which the CLI does
/// not have.
///
/// It checks what a real claim would rest on: the source key controls the
/// session's deposit account, and the amount is the claim core sized from
/// the account's CASH on People. Then it reports the claim finalized without moving anything, so
/// a run reaches `Delivered` with every core step real except the claim.
struct StandInTopUp {
    runtime: OnceLock<Weak<SigningHostRuntime>>,
    intent: OnceLock<String>,
    claimed: Mutex<HashMap<[u8; 32], u128>>,
}

impl StandInTopUp {
    fn new() -> Self {
        Self {
            runtime: OnceLock::new(),
            intent: OnceLock::new(),
            claimed: Mutex::new(HashMap::new()),
        }
    }

    /// Why the claim in `request` would fail, if it would.
    fn refusal(&self, request: &HostPaymentTopUpRequest) -> Option<&'static str> {
        let PaymentTopUpSource::PrivateKey { sr25519_secret_key } = &request.source else {
            return Some("the source is not a private key");
        };
        let Ok(secret) = schnorrkel::SecretKey::from_bytes(sr25519_secret_key) else {
            return Some("the source key is not a schnorrkel secret");
        };
        let session = self
            .runtime
            .get()
            .and_then(Weak::upgrade)
            .zip(self.intent.get())
            .and_then(|(runtime, intent)| runtime.funding_session(intent));
        let Some(session) = session else {
            return Some("no session to claim for");
        };
        if session.deposit.map(|deposit| deposit.account) != Some(secret.to_public().to_bytes()) {
            return Some("the source key does not control the deposit account");
        }
        let sized = match session.stage {
            FundingStage::Crediting { progress } => progress.claim.map(|claim| claim.amount),
            _ => None,
        };
        (sized != Some(request.amount)).then_some("the amount is not the claim core sized")
    }
}

#[async_trait]
impl TopUpPlatform for StandInTopUp {
    async fn top_up(
        &self,
        _product: &ProductContext,
        request: HostPaymentTopUpRequest,
    ) -> Result<(), HostPaymentTopUpError> {
        if let Some(reason) = self.refusal(&request) {
            println!("stand-in top-up refused: {reason}");
            return Err(HostPaymentTopUpError::InvalidSource);
        }
        let mut claimed = self.claimed.lock().expect("claims mutex poisoned");
        if claimed.contains_key(&request.id) {
            return Err(HostPaymentTopUpError::AlreadyExists);
        }
        println!(
            "stand-in top-up: would claim {} CASH units into the balance (no coins moved)",
            request.amount
        );
        claimed.insert(request.id, request.amount);
        Ok(())
    }

    fn subscribe_top_up_status(
        &self,
        _product: &ProductContext,
        id: [u8; 32],
    ) -> BoxStream<
        'static,
        Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>,
    > {
        let known = self
            .claimed
            .lock()
            .expect("claims mutex poisoned")
            .contains_key(&id);
        let status = match known {
            true => Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true }),
            false => Err(HostPaymentTopUpStatusSubscribeError::NotFound),
        };
        stream::iter([status]).boxed()
    }
}

/// What to run.
pub struct FundingCheck {
    /// Mnemonic of the identity whose funding accounts are used.
    pub mnemonic: String,
    /// Network preset.
    pub network: Network,
    /// Asset the deposit is paid in.
    pub asset: FundingAsset,
    /// CASH to credit, in its smallest units.
    pub amount: u128,
    /// Where sessions and account counters persist between runs.
    pub state_dir: PathBuf,
    /// A session to follow instead of opening a new one.
    pub intent: Option<String>,
    /// Convert what arrived of this asset instead of what was asked.
    pub accept: Option<FundingAsset>,
    /// Try a failed session again from where its funds are.
    pub retry: bool,
    /// Print the session's account seeds for a wallet, and stop.
    pub export_key: bool,
}

/// Run `check` until its session lands CASH on People or fails.
pub async fn run(
    check: FundingCheck,
    build_runtime: impl FnOnce(NetworkConfig, PathBuf) -> Result<Arc<SigningHostRuntime>>,
) -> Result<()> {
    let assets = FundingAssets::of(check.network)
        .context("no funding asset ids are known for this network")?;
    let runtime = build_runtime(check.network.config(), check.state_dir)?;
    let entropy = bip39::Mnemonic::parse(check.mnemonic.trim())
        .context("invalid mnemonic")?
        .to_entropy();
    runtime
        .activate_local_session(entropy)
        .await
        .map_err(|error| anyhow::anyhow!("activating the signer failed: {}", error.reason))?;
    runtime.set_funding_platform(Arc::new(TerminalFundingHost));
    let top_up = Arc::new(StandInTopUp::new());
    let _ = top_up.runtime.set(Arc::downgrade(&runtime));
    runtime.set_top_up_platform(top_up.clone());
    runtime.enable_funding_conversion(
        FundingNetwork {
            cash_asset_id: assets.cash,
        },
        vec![assets.cash, assets.usdt, assets.usdc],
    );

    let intent = match check.intent {
        Some(intent) => intent,
        None => open_and_assign(&runtime, &assets, check.asset, check.amount).await?,
    };
    let _ = top_up.intent.set(intent.clone());
    if check.export_key {
        return export_keys(&runtime, &intent);
    }
    if let Some(asset) = check.accept {
        let (deposit_asset, _) = assets.source(asset);
        runtime
            .accept_funding_deposit(&intent, deposit_asset)
            .await
            .map_err(|error| anyhow::anyhow!("accepting the deposit failed: {}", error.reason))?;
        println!("accepted what arrived of {deposit_asset:?}");
    }
    if check.retry {
        runtime
            .retry_funding(&intent)
            .await
            .map_err(|error| anyhow::anyhow!("retrying the session failed: {}", error.reason))?;
        println!("retrying the session");
    }
    follow(&runtime, &intent).await
}

/// Open a session and give it a deposit account, printing where to pay.
async fn open_and_assign(
    runtime: &SigningHostRuntime,
    assets: &FundingAssets,
    asset: FundingAsset,
    amount: u128,
) -> Result<String> {
    let (deposit_asset, source_id) = assets.source(asset);
    let intent = runtime
        .open_funding(FundingDirection::In, Some(amount))
        .await
        .map_err(|error| anyhow::anyhow!("opening a session failed: {}", error.reason))?
        .context("the session was dismissed")?;
    let expected = runtime
        .quote_funding_deposit(&intent, deposit_asset)
        .await
        .map_err(|error| anyhow::anyhow!("quoting the deposit failed: {}", error.reason))?;
    let account = runtime
        .assign_funding_deposit(
            &intent,
            DepositRequest {
                source_id: source_id.to_string(),
                asset: deposit_asset,
                expected,
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!("assigning a deposit account failed: {}", error.reason))?;
    println!("session  {intent}, crediting {amount} CASH units");
    println!("pay      {expected} of {deposit_asset:?} ({source_id}) on Asset Hub to");
    println!(
        "         {}",
        truapi::host_logic::product_account::product_public_key_to_address(account)
    );
    println!("         0x{}", hex::encode(account));
    println!("resume   --intent {intent}");
    Ok(intent)
}

/// Print the raw seeds of the session's deposit and refund accounts, in the
/// form a wallet imports and getcash exports: `0x` and the mini secret's hex.
fn export_keys(runtime: &SigningHostRuntime, intent: &str) -> Result<()> {
    for kind in [FundingAccountKind::Deposit, FundingAccountKind::Refund] {
        let secret = runtime
            .funding_account_secret(intent, kind)
            .map_err(|error| anyhow::anyhow!("exporting the key failed: {}", error.reason))?
            .context("no signing session is active")?;
        println!("{kind:?} seed 0x{}", hex::encode(secret));
    }
    Ok(())
}

/// Print the session's stage whenever it changes, until it settles.
async fn follow(runtime: &SigningHostRuntime, intent: &str) -> Result<()> {
    let mut last = None;
    let mut last_mismatch = None;
    let mut missing_polls = 0;
    loop {
        // Persisted sessions load in the background after the funding host
        // is installed, so a resumed one can take a moment to appear.
        let Some(session) = runtime.funding_session(intent) else {
            missing_polls += 1;
            if missing_polls > MISSING_POLLS {
                bail!("no funding session {intent}");
            }
            tokio::time::sleep(POLL).await;
            continue;
        };
        if last.as_ref() != Some(&session.stage) {
            println!("stage    {:?}", session.stage);
            last = Some(session.stage.clone());
        }
        let mismatch = session.deposit_mismatch();
        if mismatch != last_mismatch {
            if let Some(mismatch) = mismatch {
                println!(
                    "mismatch {mismatch:?}; accept with --accept or take it back with --export-key"
                );
            }
            last_mismatch = mismatch;
        }
        match session.stage {
            FundingStage::Failed {
                reason,
                resume: Some(_),
                ..
            } => bail!(
                "the session failed: {reason:?}; its funds are still held, try again with --retry"
            ),
            FundingStage::Failed { reason, .. } => bail!("the session failed: {reason:?}"),
            FundingStage::Delivered { credited, .. } => {
                println!("credited {credited} CASH units (stand-in top-up: no coins moved)");
                return Ok(());
            }
            FundingStage::Open
            | FundingStage::Converting { .. }
            | FundingStage::Converted { .. }
            | FundingStage::Crediting { .. } => {}
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(source: PaymentTopUpSource) -> HostPaymentTopUpRequest {
        HostPaymentTopUpRequest {
            into: None,
            amount: 1_000,
            source,
            id: [1; 32],
        }
    }

    // The stand-in must refuse what a real coinage engine would, or a run
    // that reaches `Delivered` proves nothing about the claim core asked for.
    #[test]
    fn the_stand_in_refuses_claims_a_real_engine_would() {
        let top_up = StandInTopUp::new();
        let secret = schnorrkel::MiniSecretKey::from_bytes(&[7; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
            .secret
            .to_bytes();

        assert_eq!(
            [
                top_up.refusal(&request(PaymentTopUpSource::Coins {
                    sr25519_secret_keys: vec![secret],
                })),
                top_up.refusal(&request(PaymentTopUpSource::PrivateKey {
                    sr25519_secret_key: [0xff; 64],
                })),
                top_up.refusal(&request(PaymentTopUpSource::PrivateKey {
                    sr25519_secret_key: secret,
                })),
            ],
            [
                Some("the source is not a private key"),
                Some("the source key is not a schnorrkel secret"),
                Some("no session to claim for"),
            ]
        );
    }
}
