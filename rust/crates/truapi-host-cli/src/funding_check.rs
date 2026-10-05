//! A real on-ramp against a live network, run from the terminal.
//!
//! The command opens a funding session on a signing host, prints the deposit
//! address its provider would pay, and follows the session while someone pays
//! that address from any funded account. It ends once the CASH lands on
//! People, or the session fails.
//!
//! Sessions and account counters live under the state directory, so a second
//! run with `--intent` picks up the same session, and no run reuses an
//! account.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use truapi::host_logic::funding::{DepositAsset, DepositRequest, FundingStage};
use truapi::latest::{FundingDirection, GenericError, HostFundingStatusSubscribeItem};
use truapi::platform::{
    FundingPlatform, FundingPresentOutcome, FundingPresentation, ProductContext, async_trait,
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
    runtime.enable_funding_conversion(FundingNetwork {
        cash_asset_id: assets.cash,
    });

    let intent = match check.intent {
        Some(intent) => intent,
        None => open_and_assign(&runtime, &assets, check.asset, check.amount).await?,
    };
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

/// Print the session's stage whenever it changes, until it settles.
async fn follow(runtime: &SigningHostRuntime, intent: &str) -> Result<()> {
    let mut last = None;
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
        match session.stage {
            FundingStage::Converted { landed } => {
                println!("landed   {landed} CASH units on People");
                return Ok(());
            }
            FundingStage::Failed { reason, .. } => bail!("the session failed: {reason:?}"),
            FundingStage::Delivered { credited, .. } => {
                println!("credited {credited} CASH units to the balance");
                return Ok(());
            }
            FundingStage::Open
            | FundingStage::Converting { .. }
            | FundingStage::Crediting { .. } => {}
        }
        tokio::time::sleep(POLL).await;
    }
}
