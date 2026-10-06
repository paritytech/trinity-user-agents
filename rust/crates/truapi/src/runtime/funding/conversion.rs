//! Turning a funding deposit into CASH on People, the way getcash does it.
//!
//! One Asset Hub transaction, signed by the deposit account, converts the
//! deposit (nothing to do for CASH, a PSM mint for an approved stablecoin) and
//! teleports the CASH to the same account on People. Every fee is paid in the
//! deposited asset, since that is all the account holds. Before submitting,
//! the transaction is dry-run on Asset Hub and the message it forwards is
//! dry-run on People, so a conversion that would trap funds is never sent.

use std::sync::Arc;

use parity_scale_codec::{Decode, Encode};
use subxt::client::OnlineClientAtBlock;
use subxt::config::substrate::SubstrateConfig;
use subxt::dynamic::{self, Value};
use subxt::ext::scale_value::{Composite, ValueDef};
use subxt::tx::ValidationResult;
use subxt::utils::Era;
use futures::future::BoxFuture;
use truapi::latest::{GenericError, TxPayloadExtension};

use crate::host_internal::extrinsic::{Sr25519Signer, build_signed_extrinsic_v4};
use super::DepositBalances;
use super::credit::CLAIM_UNIT;
use crate::host_logic::funding::{ConversionRoute, DepositAsset, DepositQuote, FundingDeposit};
use crate::runtime::statement_allowance::ChainContext;
use crate::runtime::statement_allowance::extension::{ChainState, Metadata as ExtensionMetadata};

/// XCM version every program and dry run uses.
const XCM_VERSION: u32 = 5;
/// Margin added to every fee estimate, in percent.
const FEE_MARGIN_PERCENT: u128 = 10;
/// Least CASH set aside to pay for execution on People.
const REMOTE_FEE_FLOOR: u128 = 1_000;
/// Share of the teleported CASH set aside for execution on People, in
/// percent.
const REMOTE_FEE_PERCENT: u128 = 1;
/// Mortality of a conversion transaction, in blocks.
const MORTAL_PERIOD_BLOCKS: u64 = 64;
/// PSM capacity a mint must leave spare, in percent of the amount.
const PSM_CAPACITY_MARGIN_PERCENT: u128 = 10;
/// Least PSM capacity a mint must leave spare, in CASH units.
const PSM_CAPACITY_MARGIN_FLOOR: u128 = 1_000_000;
/// Times a stablecoin swap's dry run is retried with its fee allowance
/// doubled.
const SWAP_ALLOWANCE_RETRIES: u32 = 2;
/// Headroom a pool swap's least output leaves below its quote, in percent.
const SWAP_SLIPPAGE_PERCENT: u128 = 5;
/// Parts per million in a `Permill`.
const PARTS_PER_MILLION: u128 = 1_000_000;

/// Network constants the conversion needs that the chains do not state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundingNetwork {
    /// `Assets` pallet id of CASH on Asset Hub.
    pub cash_asset_id: u32,
}

/// Signs conversions with funding deposit accounts.
pub trait FundingSigner: Send + Sync {
    /// The keypair of the `number`th deposit account for `source_id`, or
    /// `None` while no signing session is active.
    fn deposit_keypair(
        &self,
        source_id: &str,
        number: u32,
    ) -> Result<Option<schnorrkel::Keypair>, GenericError>;

    /// The reserved funding product the deposit accounts sit under, which
    /// the top-ups crediting them are made as.
    fn funding_product_id(&self) -> String;
}

/// Asset Hub and People, each pinned to its latest finalized block, with the
/// facts about them the programs need.
pub struct Chains {
    asset_hub: OnlineClientAtBlock<SubstrateConfig>,
    people: OnlineClientAtBlock<SubstrateConfig>,
    places: Places,
    /// Signed-extension metadata, loaded only when a conversion is signed.
    extensions: Option<Arc<ExtensionMetadata>>,
}

/// A signed conversion, with what tells later whether it worked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The signed transaction.
    pub extrinsic: Vec<u8>,
    /// Last Asset Hub block its mortal era admits it in.
    pub valid_until_block: u64,
    /// Least CASH it lands on People.
    pub landing: u128,
    /// Deposit it takes from the account on Asset Hub.
    pub spent: u128,
}

/// Fees a conversion pays in its deposit asset, margins included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fees {
    /// For the transaction itself.
    dispatch: u128,
    /// For executing the program on Asset Hub and delivering it to People.
    allowance: u128,
}

/// What signing a conversion needs.
#[derive(Clone, Copy)]
struct Signing<'a> {
    extensions: &'a ExtensionMetadata,
    signer: &'a Sr25519Signer,
    nonce: u32,
}

/// A signer whose signature only gives a quote's transaction its length.
fn quoting_signer() -> Sr25519Signer {
    Sr25519Signer::from_keypair(
        &schnorrkel::MiniSecretKey::from_bytes(&[1; 32])
            .expect("32 bytes are a mini secret")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519),
    )
}

/// What a conversion pass reads and does on the chains.
pub trait ConversionChains: DepositBalances {
    /// CASH `account` holds on People.
    fn landed<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>>;
    /// `account`'s next nonce on Asset Hub.
    fn nonce<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>>;
    /// The finalized Asset Hub block these reads are pinned to.
    fn finalized_block(&self) -> u64;
    /// Size, dry-run and sign the conversion of `deposit` at `nonce`.
    fn prepare<'a>(
        &'a self,
        deposit: &'a FundingDeposit,
        keypair: &'a schnorrkel::Keypair,
        nonce: u32,
    ) -> BoxFuture<'a, Result<Prepared, ConversionError>>;
}

/// Where CASH, the chains and the deposit account sit, as the programs name
/// them.
#[derive(Debug, Clone, Copy)]
struct Places {
    network: FundingNetwork,
    asset_hub_para: u32,
    people_para: u32,
    assets_pallet: u8,
}

/// Why a conversion pass could not go ahead.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ConversionError {
    /// A dry run or the transaction pool refused the conversion. Counted,
    /// since retrying the same conversion keeps failing.
    #[display("refused: {_0}")]
    Refused(String),
    /// The chains could not be read or reached. Retried on the next pass.
    #[display("{_0}")]
    Chain(String),
}

fn chain(reason: impl core::fmt::Display) -> ConversionError {
    ConversionError::Chain(reason.to_string())
}

impl Chains {
    /// Pin both chains at their latest finalized blocks.
    pub async fn at_finalized(
        asset_hub: &subxt::OnlineClient<SubstrateConfig>,
        people: &subxt::OnlineClient<SubstrateConfig>,
        network: FundingNetwork,
        signing: Option<ChainContext>,
    ) -> Result<Self, ConversionError> {
        let asset_hub = asset_hub.at_current_block().await.map_err(chain)?;
        // The cache is checked against the best block; signing at the
        // finalized one needs the same runtime's extensions.
        let extensions = match signing {
            Some(context) if context.state.spec_version == asset_hub.spec_version() => {
                Some(context.metadata)
            }
            Some(_) => return Err(chain("Asset Hub is between runtime versions")),
            None => None,
        };
        let people = people.at_current_block().await.map_err(chain)?;
        let asset_hub_para = parachain_id(&asset_hub).await?;
        let people_para = parachain_id(&people).await?;
        let assets_pallet = asset_hub
            .metadata_ref()
            .pallet_by_name("Assets")
            .ok_or_else(|| chain("Asset Hub has no Assets pallet"))?
            .call_index();
        Ok(Self {
            asset_hub,
            people,
            places: Places {
                network,
                asset_hub_para,
                people_para,
                assets_pallet,
            },
            extensions,
        })
    }

    /// The route for `expected` of `asset`: a teleport for CASH, a PSM mint
    /// for a stablecoin the PSM serves with room to spare.
    pub async fn choose_route(
        &self,
        asset: DepositAsset,
        expected: u128,
    ) -> Result<Option<ConversionRoute>, ConversionError> {
        if asset == DepositAsset::Asset(self.places.network.cash_asset_id) {
            return Ok(Some(ConversionRoute::Teleport));
        }
        if let DepositAsset::Asset(id) = asset
            && let Some(psm) = self.psm(id).await?
            && psm.serves(psm.to_internal(expected))
        {
            return Ok(Some(ConversionRoute::Psm {
                fee_ppm: psm.fee_ppm,
            }));
        }
        // A pool path counts only if it swaps enough to pay for execution on
        // People and still land something.
        match self.swap(asset, expected).await {
            Ok(swap) if swap.min_cash > 2 * REMOTE_FEE_FLOOR => Ok(Some(ConversionRoute::Pool)),
            Ok(_) | Err(ConversionError::Refused(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// `asset` the pools take to return `cash`, through the native token for
    /// a stablecoin, with the slippage headroom on each hop, or `None`
    /// without a pool path.
    async fn swap_in(&self, asset: DepositAsset, cash: u128) -> Result<Option<u128>, ConversionError> {
        let Some(native_in) = self
            .pool_quote("quote_price_tokens_for_exact_tokens", &native(), &self.places.cash(), with_slippage_room(cash))
            .await?
        else {
            return Ok(None);
        };
        match asset {
            DepositAsset::Native => Ok(Some(native_in)),
            DepositAsset::Asset(id) => {
                self.pool_quote(
                    "quote_price_tokens_for_exact_tokens",
                    &self.places.asset_location(id),
                    &native(),
                    with_slippage_room(native_in),
                )
                .await
            }
        }
    }

    /// One `AssetConversionApi` price between `give` and `want`.
    async fn pool_quote(
        &self,
        method: &str,
        give: &Value,
        want: &Value,
        amount: u128,
    ) -> Result<Option<u128>, ConversionError> {
        let quoted = call_api(
            &self.asset_hub,
            "AssetConversionApi",
            method,
            vec![give.clone(), want.clone(), Value::u128(amount), Value::bool(true)],
        )
        .await?;
        match variant_name(&quoted) {
            Some("Some") => variant_fields(&quoted)
                .and_then(|fields| fields.values().next())
                .map(as_u128)
                .transpose(),
            _ => Ok(None),
        }
    }

    /// The pool swap for `give` of `asset`, its least outputs the quotes less
    /// the slippage headroom.
    async fn swap(&self, asset: DepositAsset, give: u128) -> Result<PoolSwap, ConversionError> {
        let no_pool = || ConversionError::Refused("no pool swaps the deposit into CASH".into());
        let min_native = match asset {
            DepositAsset::Native => None,
            DepositAsset::Asset(id) => Some(less_slippage(
                self.pool_quote("quote_price_exact_tokens_for_tokens", &self.places.asset_location(id), &native(), give)
                    .await?
                    .ok_or_else(no_pool)?,
            )),
        };
        let min_cash = less_slippage(
            self.pool_quote(
                "quote_price_exact_tokens_for_tokens",
                &native(),
                &self.places.cash(),
                min_native.unwrap_or(give),
            )
            .await?
            .ok_or_else(no_pool)?,
        );
        Ok(PoolSwap {
            from: asset,
            min_native,
            min_cash,
        })
    }

    /// The route for a deposit of `asset` that credits at least `target`
    /// CASH, with the deposit it takes: the target rounded up to what a
    /// top-up claims, plus the CASH set aside to execute on People, the PSM
    /// fee, the transaction fees with their margin, and the account's
    /// minimum balance.
    ///
    /// The set-aside on People is the quote's cushion: what execution there
    /// does not spend is refunded to the account and lands as CASH, so fees
    /// that rise between the quote and the conversion still credit the
    /// target.
    pub async fn deposit_quote(
        &self,
        asset: DepositAsset,
        target: u128,
    ) -> Result<Option<DepositQuote>, ConversionError> {
        let too_large = || ConversionError::Refused("the amount is too large to quote".into());
        if target == 0 {
            return Err(ConversionError::Refused("the amount to credit is zero".into()));
        }
        let send = cash_to_teleport(target).ok_or_else(too_large)?;
        let psm = match asset {
            DepositAsset::Asset(id) if id != self.places.network.cash_asset_id => {
                self.psm(id).await?.map(|terms| (id, terms))
            }
            _ => None,
        };
        let minted = psm.and_then(|(id, terms)| {
            let internal = psm_mint_in(send, terms.fee_ppm)?;
            terms.serves(internal).then_some((id, terms, internal))
        });
        let (route, converter, converted) = if asset == DepositAsset::Asset(self.places.network.cash_asset_id) {
            (ConversionRoute::Teleport, Converter::Teleport, send)
        } else if let Some((id, terms, internal)) = minted {
            let mint = PsmMint {
                id,
                terms,
                max_fee_ppm: terms.fee_ppm,
            };
            (
                ConversionRoute::Psm {
                    fee_ppm: terms.fee_ppm,
                },
                Converter::Mint(mint),
                terms.to_external(internal),
            )
        } else {
            let Some(given) = self.swap_in(asset, send).await? else {
                return Ok(None);
            };
            let swap = PoolSwap {
                from: asset,
                min_native: matches!(asset, DepositAsset::Asset(_)).then_some(1),
                min_cash: send,
            };
            (ConversionRoute::Pool, Converter::Swap(swap), given)
        };
        let extensions = self
            .extensions
            .as_deref()
            .ok_or_else(|| chain("signing metadata was not loaded"))?;
        let signer = quoting_signer();
        let signing = Signing {
            extensions,
            signer: &signer,
            nonce: 0,
        };
        let fees = self
            .estimate_fees(signing, &[0; 32], asset, converted, converter, false)
            .await?;
        let kept = self.min_balance(asset).await?;
        let deposit = [fees.allowance, fees.dispatch, kept]
            .into_iter()
            .try_fold(converted, u128::checked_add)
            .ok_or_else(too_large)?;
        Ok(Some(DepositQuote {
            asset,
            route,
            deposit,
        }))
    }

    /// Fees a conversion of about `converted` from `account` pays in its
    /// deposit asset, with their margins: local execution from the program's
    /// weight, dispatch from the transaction's length, and delivery to People.
    ///
    /// Delivery is priced on the message the transfer forwards as the
    /// executor builds it, so a quote needs no funded account. When `funded`,
    /// the draft is also dry-run and its real forwarded message priced, and
    /// the dearer of the two counts, so a change in what the runtime forwards
    /// cannot leave a conversion short.
    async fn estimate_fees(
        &self,
        signing: Signing<'_>,
        account: &[u8; 32],
        asset: DepositAsset,
        converted: u128,
        converter: Converter,
        funded: bool,
    ) -> Result<Fees, ConversionError> {
        let fee_asset = self.places.deposit_location(asset);
        let (withdrawn, allowance) = match funded {
            true => (converted, converted / 5),
            false => (converted.saturating_mul(2), converted),
        };
        let draft = self
            .measured(self.places.conversion_call(account, withdrawn, allowance, converter.drafted())?)
            .await?;
        let local = self.local_fee(&draft.program, &fee_asset).await?;
        let cash = match converter {
            Converter::Teleport => converted,
            Converter::Mint(mint) => psm_mint_out(mint.terms.to_internal(converted), mint.terms.fee_ppm),
            Converter::Swap(swap) => swap.min_cash,
        };
        let forwarded = self.places.forwarded_to_people(account, cash);
        let mut delivery = self.delivery_fee(&forwarded, &fee_asset).await?;
        if funded {
            let real = self.dry_run(account, &draft).await?;
            delivery = delivery.max(self.delivery_fee(&real, &fee_asset).await?);
        }
        let extrinsic = self.sign(
            signing.extensions,
            signing.signer,
            &draft,
            &fee_asset,
            signing.nonce,
        )?;
        let dispatch = self.dispatch_fee(&extrinsic, &fee_asset).await?;
        Ok(Fees {
            dispatch: with_margin(dispatch),
            allowance: with_margin(local.saturating_add(delivery)),
        })
    }

    /// The PSM's terms for minting CASH against asset `id`, if it lists it.
    async fn psm(&self, id: u32) -> Result<Option<PsmTerms>, ConversionError> {
        let cash = self.places.cash();
        let external = self.places.asset_location(id);
        let storage = self.asset_hub.storage();
        let Some(instance) = fetch_value(
            &self.asset_hub,
            "Psm",
            "Psm",
            vec![cash.clone()],
        )
        .await?
        else {
            return Ok(None);
        };
        let Some(listing) = fetch_value(
            &self.asset_hub,
            "Psm",
            "ExternalAssets",
            vec![cash.clone(), external.clone()],
        )
        .await?
        else {
            return Ok(None);
        };
        let max_debt = u128_at(&instance, "max_debt")?;
        // An unset fee is the pallet's default, not zero: reading it as zero
        // would cap every mint's fee at nothing and have the PSM refuse it.
        let fee_ppm = as_u128(
            &fetch_or_default(
                &self.asset_hub,
                "Psm",
                "MintingFee",
                vec![cash.clone(), external.clone()],
            )
            .await?,
        )?;

        let suffix = external_key_suffix(&self.asset_hub, &external)?;
        let mut total_weight = 0u128;
        let mut weight = 0u128;
        let mut weights = storage
            .iter(
                dynamic::storage::<(Value, Value), Value>("Psm", "AssetCeilingWeight"),
                (cash.clone(),),
            )
            .await
            .map_err(chain)?;
        while let Some(entry) = weights.next().await {
            let entry = entry.map_err(chain)?;
            let value = as_u128(&entry.value().decode().map_err(chain)?)?;
            total_weight += value;
            if entry.key_bytes().ends_with(&suffix) {
                weight = value;
            }
        }
        let mut total_debt = 0u128;
        let mut debt = 0u128;
        let mut debts = storage
            .iter(
                dynamic::storage::<(Value, Value), Value>("Psm", "PsmDebt"),
                (cash.clone(),),
            )
            .await
            .map_err(chain)?;
        while let Some(entry) = debts.next().await {
            let entry = entry.map_err(chain)?;
            let value = as_u128(&entry.value().decode().map_err(chain)?)?;
            total_debt += value;
            if entry.key_bytes().ends_with(&suffix) {
                debt = value;
            }
        }
        let ceiling = max_debt
            .saturating_mul(weight)
            .checked_div(total_weight)
            .unwrap_or(0);
        let headroom = max_debt
            .saturating_sub(total_debt)
            .min(ceiling.saturating_sub(debt));
        Ok(Some(PsmTerms {
            minting_enabled: variant_name(field(&listing, "status")?) == Some("AllEnabled"),
            min_swap_amount: u128_at(&instance, "min_swap_amount")?,
            internal_decimals: u8::try_from(u128_at(&instance, "internal_decimals")?)
                .map_err(chain)?,
            external_decimals: u8::try_from(u128_at(&listing, "decimals")?).map_err(chain)?,
            fee_ppm: u32::try_from(fee_ppm).map_err(chain)?,
            headroom,
        }))
    }

    /// `account`'s balance of `asset` on Asset Hub.
    async fn asset_hub_balance(
        &self,
        asset: DepositAsset,
        account: &[u8; 32],
    ) -> Result<u128, ConversionError> {
        let (pallet, keys) = match asset {
            DepositAsset::Native => ("System", vec![Value::from_bytes(account)]),
            DepositAsset::Asset(id) => (
                "Assets",
                vec![Value::u128(id.into()), Value::from_bytes(account)],
            ),
        };
        let Some(entry) = fetch_value(&self.asset_hub, pallet, "Account", keys).await? else {
            return Ok(0);
        };
        match asset {
            DepositAsset::Native => u128_at(field(&entry, "data")?, "free"),
            DepositAsset::Asset(_) => u128_at(&entry, "balance"),
        }
    }

    /// The least balance of `asset` an Asset Hub account must keep.
    async fn min_balance(&self, asset: DepositAsset) -> Result<u128, ConversionError> {
        let DepositAsset::Asset(id) = asset else {
            let deposit = self
                .asset_hub
                .metadata_ref()
                .pallet_by_name("Balances")
                .and_then(|pallet| pallet.constant_by_name("ExistentialDeposit"))
                .ok_or_else(|| chain("Asset Hub declares no existential deposit"))?
                .value();
            return u128::decode(&mut &deposit[..]).map_err(chain);
        };
        let details = fetch_value(&self.asset_hub, "Assets", "Asset", vec![Value::u128(id.into())])
            .await?
            .ok_or_else(|| chain(format!("asset {id} does not exist")))?;
        u128_at(&details, "min_balance")
    }

    /// CASH `account` holds on People.
    async fn people_cash(&self, account: &[u8; 32]) -> Result<u128, ConversionError> {
        let Some(entry) = fetch_value(
            &self.people,
            "Assets",
            "Account",
            vec![self.places.cash_on_people(), Value::from_bytes(account)],
        )
        .await?
        else {
            return Ok(0);
        };
        u128_at(&entry, "balance")
    }

    /// `account`'s next nonce on Asset Hub.
    async fn account_nonce(&self, account: &[u8; 32]) -> Result<u32, ConversionError> {
        let nonce = call_api(
            &self.asset_hub,
            "AccountNonceApi",
            "account_nonce",
            vec![Value::from_bytes(account)],
        )
        .await?;
        u32::try_from(as_u128(&nonce)?).map_err(chain)
    }

    async fn prepare_conversion(
        &self,
        deposit: &FundingDeposit,
        keypair: &schnorrkel::Keypair,
        nonce: u32,
    ) -> Result<Prepared, ConversionError> {
        let signer = Sr25519Signer::from_keypair(keypair);
        let account = deposit.account;
        let fee_asset = self.places.deposit_location(deposit.asset);
        let held = self.asset_hub_balance(deposit.asset, &account).await?;
        let kept = self.min_balance(deposit.asset).await?;
        let spendable = held
            .checked_sub(kept)
            .ok_or_else(|| ConversionError::Refused("the deposit is below the minimum balance".into()))?;
        let extensions = self
            .extensions
            .as_deref()
            .ok_or_else(|| chain("signing metadata was not loaded"))?;
        let converter = match (deposit.route, deposit.asset) {
            (ConversionRoute::Teleport, _) => Converter::Teleport,
            (ConversionRoute::Psm { fee_ppm }, DepositAsset::Asset(id)) => Converter::Mint(PsmMint {
                id,
                terms: self
                    .psm(id)
                    .await?
                    .ok_or_else(|| ConversionError::Refused("the PSM no longer lists the asset".into()))?,
                max_fee_ppm: fee_ppm,
            }),
            (ConversionRoute::Psm { .. }, DepositAsset::Native) => {
                return Err(ConversionError::Refused("the PSM mints only from assets".into()));
            }
            (ConversionRoute::Pool, asset) => Converter::Swap(PoolSwap {
                from: asset,
                min_native: matches!(asset, DepositAsset::Asset(_)).then_some(1),
                min_cash: 2 * REMOTE_FEE_FLOOR,
            }),
        };

        let signing = Signing {
            extensions,
            signer: &signer,
            nonce,
        };
        let fees = self
            .estimate_fees(signing, &account, deposit.asset, spendable, converter, true)
            .await?;
        // A stablecoin swap moves its own pool before delivery is charged in
        // that stablecoin, so a large one can need more than the allowance
        // quoted beforehand: it retries with the allowance doubled, and
        // what goes unspent is refunded to the account.
        let retries = match converter {
            Converter::Swap(PoolSwap {
                min_native: Some(_), ..
            }) => SWAP_ALLOWANCE_RETRIES,
            _ => 0,
        };
        let mut allowance = fees.allowance;
        let mut first_refusal = None;
        let (call, forwarded) = loop {
            let given = spendable
                .checked_sub(fees.dispatch)
                .and_then(|left| left.checked_sub(allowance))
                .filter(|left| *left > 0);
            let Some(given) = given else {
                return Err(ConversionError::Refused(
                    first_refusal.unwrap_or_else(|| "the deposit does not cover the fees".into()),
                ));
            };
            // A swap's least outputs are quoted for what it actually gives,
            // and never below what credits the session's target.
            let converter = match converter {
                Converter::Swap(_) => {
                    let mut swap = self.swap(deposit.asset, given).await?;
                    if let Some(floor) = deposit.target.and_then(cash_to_teleport) {
                        swap.min_cash = swap.min_cash.max(floor);
                    }
                    Converter::Swap(swap)
                }
                other => other,
            };
            let call = self
                .measured(self.places.conversion_call(&account, given + allowance, allowance, converter)?)
                .await?;
            match self.dry_run_detailed(&account, &call).await? {
                Ok(forwarded) => break (call, forwarded),
                Err(refusal) if refusal.fees_short && allowance < fees.allowance << retries => {
                    first_refusal.get_or_insert(refusal.reason);
                    allowance *= 2;
                }
                Err(refusal) => {
                    return Err(ConversionError::Refused(first_refusal.unwrap_or(refusal.reason)));
                }
            }
        };
        self.dry_run_on_people(&forwarded).await?;
        Ok(Prepared {
            extrinsic: self.sign(extensions, &signer, &call, &fee_asset, nonce)?,
            valid_until_block: self.asset_hub.block_number() + MORTAL_PERIOD_BLOCKS,
            landing: call.landing,
            spent: call.spent,
        })
    }

    /// Validate a signed conversion against Asset Hub's transaction pool,
    /// which is where fee payment in the deposit asset is checked, then
    /// submit it. Only the pool's refusal is a refusal: a failed submit may
    /// still have reached the chain.
    pub async fn submit(&self, extrinsic: Vec<u8>) -> Result<(), ConversionError> {
        let transaction = self.asset_hub.tx().from_bytes(extrinsic);
        match transaction.validate().await.map_err(chain)? {
            ValidationResult::Valid(_) => {}
            invalid => {
                return Err(ConversionError::Refused(format!(
                    "Asset Hub rejects the transaction: {invalid:?}"
                )));
            }
        }
        transaction.submit().await.map(|_| ()).map_err(chain)
    }

    /// Dry-run `call` from `account` on Asset Hub, returning the message it
    /// forwards to People.
    async fn dry_run(
        &self,
        account: &[u8; 32],
        call: &ConversionCall,
    ) -> Result<Value, ConversionError> {
        self.dry_run_detailed(account, call)
            .await?
            .map_err(|refusal| ConversionError::Refused(refusal.reason))
    }

    /// [`Self::dry_run`], telling a refusal for want of fees apart, which is
    /// the one a larger fee allowance can cure.
    async fn dry_run_detailed(
        &self,
        account: &[u8; 32],
        call: &ConversionCall,
    ) -> Result<Result<Value, DryRunRefusal>, ConversionError> {
        let refused = |reason: String| {
            Ok(Err(DryRunRefusal {
                reason,
                fees_short: false,
            }))
        };
        let origin = Value::unnamed_variant(
            "system",
            [Value::unnamed_variant("Signed", [Value::from_bytes(account)])],
        );
        let effects = ok(call_api(
            &self.asset_hub,
            "DryRunApi",
            "dry_run_call",
            vec![origin, call.runtime_call().value(), Value::u128(XCM_VERSION.into())],
        )
        .await?)?;
        let execution = field(&effects, "execution_result")?;
        if variant_name(execution) != Some("Ok") {
            let names = module_error_names(self.asset_hub.metadata_ref(), execution);
            return Ok(Err(DryRunRefusal {
                reason: format!("dry run failed on Asset Hub: {} ({execution})", names.join(" / ")),
                fees_short: names.iter().any(|name| name == "NotHoldingFees"),
            }));
        }
        let events = field(&effects, "emitted_events")?;
        if mentions_variant(events, "AssetsTrapped") {
            return refused("the conversion would trap assets on Asset Hub".into());
        }
        let forwarded = field(&effects, "forwarded_xcms")?;
        let to_people = items(forwarded).into_iter().find_map(|entry| {
            let [destination, messages] = items(entry)[..] else {
                return None;
            };
            (parachain_of(destination) == Some(self.places.people_para))
                .then(|| items(messages).first().map(|message| (*message).clone()))
                .flatten()
        });
        match to_people {
            Some(message) => Ok(Ok(message)),
            None => refused("the conversion forwards nothing to People".into()),
        }
    }

    /// Dry-run the forwarded `message` on People as Asset Hub sends it.
    async fn dry_run_on_people(&self, message: &Value) -> Result<(), ConversionError> {
        let origin = versioned(location(
            1,
            vec![junction("Parachain", Value::u128(self.places.asset_hub_para.into()))],
        ));
        let effects = ok(call_api(
            &self.people,
            "DryRunApi",
            "dry_run_xcm",
            vec![origin, message.clone()],
        )
        .await?)?;
        let outcome = field(&effects, "execution_result")?;
        if variant_name(outcome) != Some("Complete") {
            return Err(ConversionError::Refused(format!(
                "the message would not complete on People: {outcome}"
            )));
        }
        if mentions_variant(field(&effects, "emitted_events")?, "AssetsTrapped") {
            return Err(ConversionError::Refused("the conversion would trap assets on People".into()));
        }
        Ok(())
    }

    /// `call` with its execute's weight limit set to what its program weighs.
    async fn measured(&self, call: ConversionCall) -> Result<ConversionCall, ConversionError> {
        let measured = self.program_weight(&call.program).await?;
        Ok(ConversionCall {
            max_weight: measured,
            ..call
        })
    }

    async fn program_weight(&self, program: &[Value]) -> Result<Value, ConversionError> {
        ok(call_api(
            &self.asset_hub,
            "XcmPaymentApi",
            "query_xcm_weight",
            vec![versioned(Value::unnamed_composite(program.to_vec()))],
        )
        .await?)
    }

    /// What executing `program` locally costs, in `fee_asset`.
    async fn local_fee(&self, program: &[Value], fee_asset: &Value) -> Result<u128, ConversionError> {
        let weight = self.program_weight(program).await?;
        as_u128(&ok(call_api(
            &self.asset_hub,
            "XcmPaymentApi",
            "query_weight_to_asset_fee",
            vec![weight, versioned(fee_asset.clone())],
        )
        .await?)?)
    }

    /// What delivering `message` to People costs, in `fee_asset`.
    async fn delivery_fee(&self, message: &Value, fee_asset: &Value) -> Result<u128, ConversionError> {
        let people = versioned(location(
            1,
            vec![junction("Parachain", Value::u128(self.places.people_para.into()))],
        ));
        let fees = ok(call_api(
            &self.asset_hub,
            "XcmPaymentApi",
            "query_delivery_fees",
            vec![people, message.clone(), versioned(fee_asset.clone())],
        )
        .await?)?;
        Ok(fungible_total(unversioned(&fees)?))
    }

    /// What dispatching `extrinsic` costs, in `fee_asset`.
    async fn dispatch_fee(&self, extrinsic: &[u8], fee_asset: &Value) -> Result<u128, ConversionError> {
        let unprefixed = Vec::<u8>::decode(&mut &extrinsic[..]).map_err(chain)?;
        let length = u32::try_from(extrinsic.len()).map_err(chain)?;
        let info = call_api(
            &self.asset_hub,
            "TransactionPaymentApi",
            "query_info",
            vec![Value::from_bytes(&unprefixed), Value::u128(length.into())],
        )
        .await?;
        let native_fee = u128_at(&info, "partial_fee")?;
        if fee_asset == &native() {
            return Ok(native_fee);
        }
        let quoted = call_api(
            &self.asset_hub,
            "AssetConversionApi",
            "quote_price_tokens_for_exact_tokens",
            vec![fee_asset.clone(), native(), Value::u128(native_fee), Value::bool(true)],
        )
        .await?;
        let quoted = variant_fields(&quoted)
            .filter(|_| variant_name(&quoted) == Some("Some"))
            .and_then(|fields| fields.values().next())
            .ok_or_else(|| ConversionError::Refused("no pool prices the fee asset".into()))?;
        as_u128(quoted)
    }

    /// Sign `call` with the deposit account at `nonce`, mortal from this
    /// block, paying fees in `fee_asset`.
    fn sign(
        &self,
        extensions: &ExtensionMetadata,
        signer: &Sr25519Signer,
        call: &ConversionCall,
        fee_asset: &Value,
        nonce: u32,
    ) -> Result<Vec<u8>, ConversionError> {
        let call_data = call.runtime_call().encode(self.asset_hub.metadata_ref())?;
        let genesis: [u8; 32] = self
            .asset_hub
            .genesis_hash()
            .ok_or_else(|| chain("Asset Hub genesis unknown"))?
            .0;
        let state = ChainState {
            spec_version: self.asset_hub.spec_version(),
            transaction_version: self.asset_hub.transaction_version(),
            genesis_hash: genesis,
            nonce,
            restrict_origins: false,
        };
        let payment = self.charge_asset_tx_payment(fee_asset)?;
        let block_hash = self.asset_hub.block_hash().0;
        let era = Era::mortal(MORTAL_PERIOD_BLOCKS, self.asset_hub.block_number()).encode();
        let extensions: Vec<TxPayloadExtension> = extensions
            .extension_ids()
            .into_iter()
            .zip(extensions.encode_signed_extensions(&state))
            .map(|(id, encoded)| {
                let (extra, additional_signed) = match id {
                    "CheckMortality" => (era.clone(), block_hash.to_vec()),
                    "ChargeAssetTxPayment" => (payment.clone(), encoded.additional_signed),
                    _ => (encoded.extra, encoded.additional_signed),
                };
                TxPayloadExtension {
                    id: id.to_string(),
                    extra,
                    additional_signed,
                }
            })
            .collect();
        Ok(build_signed_extrinsic_v4(signer, &call_data, &extensions))
    }
}

impl Places {
    /// CASH as Asset Hub names it.
    fn cash(&self) -> Value {
        self.asset_location(self.network.cash_asset_id)
    }

    /// An `Assets` pallet asset as Asset Hub names it.
    fn asset_location(&self, id: u32) -> Value {
        location(
            0,
            vec![
                junction("PalletInstance", Value::u128(self.assets_pallet.into())),
                junction("GeneralIndex", Value::u128(id.into())),
            ],
        )
    }

    /// The deposited asset as Asset Hub names it.
    fn deposit_location(&self, asset: DepositAsset) -> Value {
        match asset {
            DepositAsset::Native => native(),
            DepositAsset::Asset(id) => self.asset_location(id),
        }
    }

    /// CASH as People names it.
    fn cash_on_people(&self) -> Value {
        location(
            1,
            vec![
                junction("Parachain", Value::u128(self.asset_hub_para.into())),
                junction("PalletInstance", Value::u128(self.assets_pallet.into())),
                junction("GeneralIndex", Value::u128(self.network.cash_asset_id.into())),
            ],
        )
    }

    /// The call converting `withdrawn` of `asset`, of which `allowance` pays
    /// for local execution and delivery.
    fn conversion_call(
        &self,
        account: &[u8; 32],
        withdrawn: u128,
        allowance: u128,
        converter: Converter,
    ) -> Result<ConversionCall, ConversionError> {
        let converted = withdrawn
            .checked_sub(allowance)
            .ok_or_else(|| ConversionError::Refused("the fee allowance exceeds the deposit".into()))?;
        let mut exchanges = Vec::new();
        let (cash, withdrawn_assets) = match converter {
            Converter::Teleport => (converted, vec![self.asset(&self.cash(), withdrawn)]),
            Converter::Swap(swap) => {
                let from = self.deposit_location(swap.from);
                let exchange = |give: Value, want: Value| {
                    Value::named_variant(
                        "ExchangeAsset",
                        [
                            ("give", give),
                            ("want", Value::unnamed_composite([want])),
                            ("maximal", Value::bool(true)),
                        ],
                    )
                };
                let definite = |asset: Value| {
                    Value::unnamed_variant("Definite", [Value::unnamed_composite([asset])])
                };
                let cash_out = self.asset(&self.cash(), swap.min_cash);
                match swap.min_native {
                    None => exchanges.push(exchange(definite(self.asset(&from, converted)), cash_out)),
                    Some(min_native) => {
                        exchanges.push(exchange(
                            definite(self.asset(&from, converted)),
                            self.asset(&native(), min_native),
                        ));
                        let all_native = Value::unnamed_variant(
                            "Wild",
                            [Value::named_variant(
                                "AllOf",
                                [("id", native()), ("fun", Value::unnamed_variant("Fungible", []))],
                            )],
                        );
                        exchanges.push(exchange(all_native, cash_out));
                    }
                }
                (swap.min_cash, vec![self.asset(&from, withdrawn)])
            }
            Converter::Mint(mint) => {
                let internal = mint.terms.to_internal(converted);
                if internal < mint.terms.min_swap_amount {
                    return Err(ConversionError::Refused(
                        "the deposit is below the PSM's minimum swap after fees".into(),
                    ));
                }
                let minted = psm_mint_out(internal, mint.terms.fee_ppm);
                // `Assets` must be sorted, and both sit under one pallet, so
                // the general index decides the order.
                let mut assets = vec![
                    (mint.id, self.asset(&self.asset_location(mint.id), allowance)),
                    (self.network.cash_asset_id, self.asset(&self.cash(), minted)),
                ];
                assets.sort_by_key(|(index, _)| *index);
                (minted, assets.into_iter().map(|(_, asset)| asset).collect())
            }
        };
        let remote_fee = remote_fee(cash);
        if remote_fee >= cash {
            return Err(ConversionError::Refused(
                "the deposit does not cover the fee on People".into(),
            ));
        }
        let beneficiary = beneficiary(account);
        let everything = || {
            Value::unnamed_variant(
                "Wild",
                [Value::unnamed_variant("AllCounted", [Value::u128(1)])],
            )
        };
        let deposit_everything = || {
            Value::named_variant(
                "DepositAsset",
                [
                    ("assets", everything()),
                    ("beneficiary", beneficiary.clone()),
                ],
            )
        };
        let teleport = |filter: Value| Value::unnamed_variant("Teleport", [filter]);
        let fee_asset = match converter {
            Converter::Teleport => self.asset(&self.cash(), allowance),
            Converter::Mint(mint) => self.asset(&self.asset_location(mint.id), allowance),
            Converter::Swap(swap) => self.asset(&self.deposit_location(swap.from), allowance),
        };
        let mut program = vec![
            Value::unnamed_variant("WithdrawAsset", [Value::unnamed_composite(withdrawn_assets)]),
            Value::named_variant("PayFees", [("asset", fee_asset)]),
        ];
        program.extend(exchanges);
        program.extend([
            Value::named_variant(
                "InitiateTransfer",
                [
                    (
                        "destination",
                        location(1, vec![junction("Parachain", Value::u128(self.people_para.into()))]),
                    ),
                    (
                        "remote_fees",
                        Value::unnamed_variant(
                            "Some",
                            [teleport(Value::unnamed_variant(
                                "Definite",
                                [Value::unnamed_composite([self.asset(&self.cash(), remote_fee)])],
                            ))],
                        ),
                    ),
                    ("preserve_origin", Value::bool(false)),
                    ("assets", Value::unnamed_composite([teleport(everything())])),
                    (
                        "remote_xcm",
                        Value::unnamed_composite([
                            Value::unnamed_variant("RefundSurplus", []),
                            deposit_everything(),
                        ]),
                    ),
                ],
            ),
            Value::unnamed_variant("RefundSurplus", []),
            deposit_everything(),
        ]);
        let mint = match converter {
            Converter::Mint(mint) => Some(mint),
            Converter::Teleport | Converter::Swap(_) => None,
        };
        let mint_call = mint.map(|mint| {
            RuntimeCall::new(
                "Psm",
                "mint",
                vec![
                    ("internal_asset", self.cash()),
                    ("external_asset", self.asset_location(mint.id)),
                    ("external_amount", Value::u128(converted)),
                    ("max_fee", Value::u128(mint.max_fee_ppm.into())),
                ],
            )
        });
        Ok(ConversionCall {
            program,
            mint: mint_call,
            max_weight: weight(0, 0),
            landing: cash - remote_fee,
            spent: withdrawn,
        })
    }

    /// The message a teleport of `cash` forwards to People, as Asset Hub's
    /// executor builds it from the program: what prices its delivery before
    /// a dry run can produce the real one.
    fn forwarded_to_people(&self, account: &[u8; 32], cash: u128) -> Value {
        let remote_fee = remote_fee(cash);
        let on_people = |amount| self.asset(&self.cash_on_people(), amount);
        versioned(Value::unnamed_composite([
            Value::unnamed_variant(
                "ReceiveTeleportedAsset",
                [Value::unnamed_composite([on_people(remote_fee)])],
            ),
            Value::named_variant("PayFees", [("asset", on_people(remote_fee))]),
            Value::unnamed_variant(
                "ReceiveTeleportedAsset",
                [Value::unnamed_composite([on_people(cash.saturating_sub(remote_fee))])],
            ),
            Value::unnamed_variant("ClearOrigin", []),
            Value::unnamed_variant("RefundSurplus", []),
            Value::named_variant(
                "DepositAsset",
                [
                    (
                        "assets",
                        Value::unnamed_variant(
                            "Wild",
                            [Value::unnamed_variant("AllCounted", [Value::u128(1)])],
                        ),
                    ),
                    ("beneficiary", beneficiary(account)),
                ],
            ),
            Value::unnamed_variant("SetTopic", [Value::from_bytes([0; 32])]),
        ]))
    }

    fn asset(&self, id: &Value, amount: u128) -> Value {
        Value::named_composite([
            ("id", id.clone()),
            ("fun", Value::unnamed_variant("Fungible", [Value::u128(amount)])),
        ])
    }
}

/// How a conversion turns the deposit into CASH before the teleport.
#[derive(Debug, Clone, Copy)]
enum Converter {
    /// The deposit is CASH.
    Teleport,
    /// A PSM mint, batched ahead of the program.
    Mint(PsmMint),
    /// Pool swaps inside the program.
    Swap(PoolSwap),
}

impl Converter {
    /// The converter a fee draft runs: a swap's least outputs set just above
    /// what executing on People needs, since a draft is weighed and
    /// dry-run, not executed for value, and any real deposit swaps for more.
    fn drafted(self) -> Self {
        match self {
            Self::Swap(swap) => Self::Swap(PoolSwap {
                min_native: swap.min_native.map(|_| 1),
                min_cash: 2 * REMOTE_FEE_FLOOR,
                ..swap
            }),
            other => other,
        }
    }
}

/// Pool swaps from the deposit asset to CASH: straight for the native token,
/// through it for a stablecoin, each giving all of what it holds.
#[derive(Debug, Clone, Copy)]
struct PoolSwap {
    /// The deposit asset given.
    from: DepositAsset,
    /// Least native token the first hop returns, for a stablecoin.
    min_native: Option<u128>,
    /// Least CASH the last hop returns.
    min_cash: u128,
}

/// A PSM mint ahead of the teleport.
#[derive(Debug, Clone, Copy)]
struct PsmMint {
    /// The stablecoin minted against.
    id: u32,
    /// The PSM's current terms, which size the mint.
    terms: PsmTerms,
    /// The fee the route was chosen at, the most the mint may charge.
    max_fee_ppm: u32,
}

impl DepositBalances for Chains {
    fn balance<'a>(
        &'a self,
        asset: DepositAsset,
        account: &'a [u8; 32],
    ) -> BoxFuture<'a, Result<u128, GenericError>> {
        Box::pin(async move {
            super::within_chain_timeout(self.asset_hub_balance(asset, account))
                .await?
                .map_err(|error| GenericError {
                    reason: error.to_string(),
                })
        })
    }
}

impl ConversionChains for Chains {
    fn landed<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>> {
        Box::pin(self.people_cash(account))
    }

    fn nonce<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>> {
        Box::pin(self.account_nonce(account))
    }

    fn finalized_block(&self) -> u64 {
        self.asset_hub.block_number()
    }

    fn prepare<'a>(
        &'a self,
        deposit: &'a FundingDeposit,
        keypair: &'a schnorrkel::Keypair,
        nonce: u32,
    ) -> BoxFuture<'a, Result<Prepared, ConversionError>> {
        Box::pin(self.prepare_conversion(deposit, keypair, nonce))
    }
}

/// The PSM's terms for one stablecoin.
#[derive(Debug, Clone, Copy)]
struct PsmTerms {
    minting_enabled: bool,
    min_swap_amount: u128,
    internal_decimals: u8,
    external_decimals: u8,
    fee_ppm: u32,
    /// CASH the PSM can still mint against this stablecoin.
    headroom: u128,
}

impl PsmTerms {
    /// Whether the PSM mints `amount` CASH: minting is on, it is at least the
    /// minimum swap, and it leaves the capacity margin spare.
    fn serves(self, amount: u128) -> bool {
        let margin = (amount * PSM_CAPACITY_MARGIN_PERCENT / 100).max(PSM_CAPACITY_MARGIN_FLOOR);
        self.minting_enabled
            && amount >= self.min_swap_amount
            && self.headroom >= amount.saturating_add(margin)
    }

    /// `amount` CASH in the stablecoin's units, rounded up.
    fn to_external(self, amount: u128) -> u128 {
        let (internal, external) = (u32::from(self.internal_decimals), u32::from(self.external_decimals));
        if external >= internal {
            amount.saturating_mul(10u128.pow(external - internal))
        } else {
            amount.div_ceil(10u128.pow(internal - external))
        }
    }

    /// `amount` of the stablecoin in CASH units, rounded down.
    fn to_internal(self, amount: u128) -> u128 {
        let (internal, external) = (u32::from(self.internal_decimals), u32::from(self.external_decimals));
        if internal >= external {
            amount.saturating_mul(10u128.pow(internal - external))
        } else {
            amount / 10u128.pow(external - internal)
        }
    }
}

/// CASH the PSM must be given, in CASH units, to mint `out` at `fee_ppm`,
/// or `None` when no amount does.
fn psm_mint_in(out: u128, fee_ppm: u32) -> Option<u128> {
    let kept = PARTS_PER_MILLION.checked_sub(u128::from(fee_ppm)).filter(|kept| *kept > 0)?;
    let mut amount = out.checked_mul(PARTS_PER_MILLION)?.div_ceil(kept);
    while psm_mint_out(amount, fee_ppm) < out {
        amount = amount.checked_add(1)?;
    }
    Some(amount)
}

/// The least CASH to teleport so that at least `target` lands on People
/// after the CASH set aside to execute there, or `None` on overflow.
fn teleported_for(target: u128) -> Option<u128> {
    let lands = |send: u128| send.saturating_sub(remote_fee(send)) >= target;
    let mut send = target.checked_add(
        target
            .div_ceil(100 / REMOTE_FEE_PERCENT - 1)
            .max(REMOTE_FEE_FLOOR),
    )?;
    while !lands(send) {
        send = send.checked_add(1)?;
    }
    while send > target && lands(send - 1) {
        send -= 1;
    }
    Some(send)
}

/// The native token, as Asset Hub names it.
fn native() -> Value {
    location(1, Vec::new())
}

/// `amount` less the slippage headroom: a swap's least output.
fn less_slippage(amount: u128) -> u128 {
    let kept = 100 - SWAP_SLIPPAGE_PERCENT;
    amount / 100 * kept + amount % 100 * kept / 100
}

/// An output to ask the pools for so that, less the slippage headroom, it
/// still covers `amount`.
fn with_slippage_room(amount: u128) -> u128 {
    amount.saturating_mul(100).div_ceil(100 - SWAP_SLIPPAGE_PERCENT)
}

/// CASH to teleport so that a top-up of `target`, rounded up to what one
/// claims, lands on People, or `None` on overflow.
fn cash_to_teleport(target: u128) -> Option<u128> {
    teleported_for(target.div_ceil(CLAIM_UNIT).checked_mul(CLAIM_UNIT)?)
}

/// Why a dry run refused a conversion, and whether more fees would cure it.
#[derive(Debug)]
struct DryRunRefusal {
    reason: String,
    fees_short: bool,
}

/// The error names a failed dispatch's module error decodes to through
/// `metadata`: the pallet error, then any enum inside it, such
/// as the XCM error a failed local execution carries.
fn module_error_names(metadata: &subxt::Metadata, execution: &Value) -> Vec<String> {
    let Some(module) = find_variant(execution, "Module") else {
        return Vec::new();
    };
    let Some(pallet_index) = field(module, "index").ok().and_then(|index| as_u128(index).ok()) else {
        return Vec::new();
    };
    let bytes: Vec<u8> = field(module, "error")
        .map(|error| {
            items(unwrap_newtype(error))
                .into_iter()
                .filter_map(|byte| as_u128(byte).ok().and_then(|byte| u8::try_from(byte).ok()))
                .collect()
        })
        .unwrap_or_default();
    let Some(pallet) = u8::try_from(pallet_index)
        .ok()
        .and_then(|index| metadata.pallet_by_error_index(index))
    else {
        return Vec::new();
    };
    let Some(variant) = bytes.first().and_then(|byte| pallet.error_variant_by_index(*byte)) else {
        return Vec::new();
    };
    let mut names = vec![variant.name.clone()];
    let mut cursor = 1;
    for field in &variant.fields {
        let Some(ty) = metadata.types().resolve(field.ty.id) else {
            break;
        };
        match &ty.type_def {
            scale_info::TypeDef::Variant(inner) => {
                if let Some(name) = bytes
                    .get(cursor)
                    .and_then(|byte| inner.variants.iter().find(|variant| variant.index == *byte))
                {
                    names.push(name.name.clone());
                }
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    names
}

/// The first variant named `name` anywhere in `value`.
fn find_variant<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    match &value.value {
        ValueDef::Variant(variant) if variant.name == name => Some(value),
        ValueDef::Variant(variant) => variant.values.values().find_map(|inner| find_variant(inner, name)),
        ValueDef::Composite(composite) => composite.values().find_map(|inner| find_variant(inner, name)),
        _ => None,
    }
}

/// CASH set aside for execution on People out of `cash` teleported.
fn remote_fee(cash: u128) -> u128 {
    (cash / 100 * REMOTE_FEE_PERCENT).max(REMOTE_FEE_FLOOR)
}

/// CASH the PSM mints for `amount` in CASH units at `fee_ppm`.
fn psm_mint_out(amount: u128, fee_ppm: u32) -> u128 {
    let fee = (amount * u128::from(fee_ppm)).div_ceil(PARTS_PER_MILLION);
    amount.saturating_sub(fee)
}

/// `fee` plus the fee margin, rounded up.
fn with_margin(fee: u128) -> u128 {
    fee.saturating_add((fee * FEE_MARGIN_PERCENT).div_ceil(100))
}

impl Chains {
    /// `ChargeAssetTxPayment`'s bytes for no tip and fees in `fee_asset`,
    /// encoded against the type the runtime declares for it.
    fn charge_asset_tx_payment(&self, fee_asset: &Value) -> Result<Vec<u8>, ConversionError> {
        use subxt::ext::scale_encode::EncodeAsType;
        let metadata = self.asset_hub.metadata_ref();
        let extension = metadata
            .extrinsic()
            .transaction_extensions_to_use_for_encoding()
            .find(|extension| extension.identifier() == "ChargeAssetTxPayment")
            .ok_or_else(|| chain("Asset Hub does not charge fees in assets"))?;
        // Fees in the native token are the default and name no asset.
        let asset_id = if fee_asset == &native() {
            Value::unnamed_variant("None", [])
        } else {
            Value::unnamed_variant("Some", [fee_asset.clone()])
        };
        Value::named_composite([("tip", Value::u128(0)), ("asset_id", asset_id)])
        .encode_as_type(extension.extra_ty(), metadata.types())
        .map_err(chain)
    }
}

/// A conversion: the XCM program it executes, the PSM mint ahead of it if
/// any, the program's weight limit, and what it moves.
struct ConversionCall {
    program: Vec<Value>,
    mint: Option<RuntimeCall>,
    max_weight: Value,
    /// Least CASH it lands on People.
    landing: u128,
    /// Deposit it takes from the account on Asset Hub.
    spent: u128,
}

impl ConversionCall {
    /// The call to sign: the execute alone, or batched after the mint.
    fn runtime_call(&self) -> RuntimeCall {
        let execute = RuntimeCall::new(
            "PolkadotXcm",
            "execute",
            vec![
                ("message", versioned(Value::unnamed_composite(self.program.clone()))),
                ("max_weight", self.max_weight.clone()),
            ],
        );
        match &self.mint {
            None => execute,
            Some(mint) => RuntimeCall::new(
                "Utility",
                "batch_all",
                vec![(
                    "calls",
                    Value::unnamed_composite([mint.value(), execute.value()]),
                )],
            ),
        }
    }
}

/// A runtime call built from metadata names.
#[derive(Clone)]
struct RuntimeCall {
    pallet: &'static str,
    name: &'static str,
    fields: Vec<(&'static str, Value)>,
}

impl RuntimeCall {
    fn new(pallet: &'static str, name: &'static str, fields: Vec<(&'static str, Value)>) -> Self {
        Self { pallet, name, fields }
    }

    /// The call as a `RuntimeCall` value.
    fn value(&self) -> Value {
        Value::unnamed_variant(
            self.pallet,
            [Value::named_variant(self.name, self.fields.clone())],
        )
    }

    /// The call's SCALE bytes, encoded against `metadata`.
    fn encode(&self, metadata: &subxt::Metadata) -> Result<Vec<u8>, ConversionError> {
        subxt::ext::frame_decode::extrinsics::encode_call_data(
            self.pallet,
            self.name,
            &Value::named_composite(self.fields.clone()),
            metadata,
            metadata.types(),
        )
        .map_err(chain)
    }
}

/// `account` on the chain a message executes on.
fn beneficiary(account: &[u8; 32]) -> Value {
    location(
        0,
        vec![Value::named_variant(
            "AccountId32",
            [
                ("network", Value::unnamed_variant("None", [])),
                ("id", Value::from_bytes(account)),
            ],
        )],
    )
}

/// A location `parents` up with `junctions` below.
fn location(parents: u8, junctions: Vec<Value>) -> Value {
    let interior = match junctions.len() {
        0 => Value::unnamed_variant("Here", []),
        count => Value::unnamed_variant(format!("X{count}"), [Value::unnamed_composite(junctions)]),
    };
    Value::named_composite([("parents", Value::u128(parents.into())), ("interior", interior)])
}

fn junction(name: &'static str, value: Value) -> Value {
    Value::unnamed_variant(name, [value])
}

fn weight(ref_time: u64, proof_size: u64) -> Value {
    Value::named_composite([
        ("ref_time", Value::u128(ref_time.into())),
        ("proof_size", Value::u128(proof_size.into())),
    ])
}

/// `value` as XCM version 5.
fn versioned(value: Value) -> Value {
    Value::unnamed_variant(format!("V{XCM_VERSION}"), [value])
}

/// The value inside a versioned XCM type.
fn unversioned<T>(value: &subxt::ext::scale_value::Value<T>) -> Result<&subxt::ext::scale_value::Value<T>, ConversionError> {
    variant_fields(value)
        .and_then(|fields| fields.values().next())
        .ok_or_else(|| chain("expected a versioned XCM value"))
}

/// The SCALE bytes of `value` as the type of the first key of `pallet.item`.
fn encode_as(at: &OnlineClientAtBlock<SubstrateConfig>, value: &Value, pallet: &str, item: &str) -> Result<Vec<u8>, ConversionError> {
    use subxt::ext::scale_encode::EncodeAsType;
    let metadata = at.metadata_ref();
    let entry = metadata
        .pallet_by_name(pallet)
        .and_then(|pallet| pallet.storage())
        .and_then(|storage| storage.entry_by_name(item))
        .ok_or_else(|| chain(format!("{pallet}.{item} not in metadata")))?;
    let key_type = entry
        .keys()
        .next()
        .ok_or_else(|| chain(format!("{pallet}.{item} has no key")))?
        .key_id;
    value.encode_as_type(key_type, metadata.types()).map_err(chain)
}

/// The SCALE bytes `external` contributes at the end of a `Psm` double-map
/// key: its `Blake2_128Concat` hash.
fn external_key_suffix(at: &OnlineClientAtBlock<SubstrateConfig>, external: &Value) -> Result<Vec<u8>, ConversionError> {
    let encoded = encode_as(at, external, "Psm", "Psm")?;
    Ok(super::super::statement_allowance::blake2_128_concat(&encoded))
}

/// The parachain id a versioned location names, if it is `../Parachain(id)`.
fn parachain_of<T>(location: &subxt::ext::scale_value::Value<T>) -> Option<u32> {
    let location = unversioned(location).ok()?;
    let interior = field(location, "interior").ok()?;
    let junctions = variant_fields(interior)?.values().next()?;
    let parachain = *items(junctions).first()?;
    (variant_name(parachain) == Some("Parachain"))
        .then(|| variant_fields(parachain)?.values().next().and_then(|id| as_u128(id).ok()))
        .flatten()
        .and_then(|id| u32::try_from(id).ok())
}

async fn parachain_id(at: &OnlineClientAtBlock<SubstrateConfig>) -> Result<u32, ConversionError> {
    let id = fetch_value(at, "ParachainInfo", "ParachainId", Vec::new())
        .await?
        .ok_or_else(|| chain("the chain names no parachain id"))?;
    u32::try_from(as_u128(unwrap_newtype(&id))?).map_err(chain)
}

async fn fetch_value(
    at: &OnlineClientAtBlock<SubstrateConfig>,
    pallet: &str,
    item: &str,
    keys: Vec<Value>,
) -> Result<Option<Value>, ConversionError> {
    let address = dynamic::storage::<Vec<Value>, Value>(pallet, item);
    match at.storage().try_fetch(address, keys).await.map_err(chain)? {
        Some(value) => value.decode().map(Some).map_err(chain),
        None => Ok(None),
    }
}

/// The value of `pallet.item` at `keys`, or the default the runtime declares
/// for it when unset.
async fn fetch_or_default(
    at: &OnlineClientAtBlock<SubstrateConfig>,
    pallet: &str,
    item: &str,
    keys: Vec<Value>,
) -> Result<Value, ConversionError> {
    let address = dynamic::storage::<Vec<Value>, Value>(pallet, item);
    at.storage()
        .fetch(address, keys)
        .await
        .map_err(chain)?
        .decode()
        .map_err(chain)
}

async fn call_api(
    at: &OnlineClientAtBlock<SubstrateConfig>,
    api: &str,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, ConversionError> {
    at.runtime_apis()
        .call(dynamic::runtime_api_call::<_, Value>(api, method, args))
        .await
        .map_err(chain)
}

/// The `Ok` side of a `Result` value, or a refusal naming the error.
fn ok(value: Value) -> Result<Value, ConversionError> {
    match &value.value {
        ValueDef::Variant(result) if result.name == "Ok" => result
            .values
            .values()
            .next()
            .cloned()
            .ok_or_else(|| chain("empty Ok")),
        _ => Err(ConversionError::Refused(value.to_string())),
    }
}

fn field<'a, T>(
    value: &'a subxt::ext::scale_value::Value<T>,
    name: &str,
) -> Result<&'a subxt::ext::scale_value::Value<T>, ConversionError> {
    use subxt::ext::scale_value::At;
    value.at(name).ok_or_else(|| chain(format!("missing field {name}")))
}

fn u128_at<T>(value: &subxt::ext::scale_value::Value<T>, name: &str) -> Result<u128, ConversionError> {
    as_u128(field(value, name)?)
}

/// `value` read through any single-field wrappers.
fn unwrap_newtype<T>(mut value: &subxt::ext::scale_value::Value<T>) -> &subxt::ext::scale_value::Value<T> {
    while let ValueDef::Composite(composite) = &value.value {
        let mut values = composite.values();
        match (values.next(), values.next()) {
            (Some(inner), None) => value = inner,
            _ => break,
        }
    }
    value
}

fn as_u128<T>(value: &subxt::ext::scale_value::Value<T>) -> Result<u128, ConversionError> {
    unwrap_newtype(value)
        .as_u128()
        .ok_or_else(|| chain(format!("expected a number, got {value}")))
}

fn variant_name<T>(value: &subxt::ext::scale_value::Value<T>) -> Option<&str> {
    match &value.value {
        ValueDef::Variant(variant) => Some(variant.name.as_str()),
        _ => None,
    }
}

fn variant_fields<T>(value: &subxt::ext::scale_value::Value<T>) -> Option<&Composite<T>> {
    match &value.value {
        ValueDef::Variant(variant) => Some(&variant.values),
        _ => None,
    }
}

/// The direct items of a sequence, tuple or composite value.
fn items<T>(value: &subxt::ext::scale_value::Value<T>) -> Vec<&subxt::ext::scale_value::Value<T>> {
    match &value.value {
        ValueDef::Composite(composite) => composite.values().collect(),
        _ => Vec::new(),
    }
}

/// The sum of every `Fungible` amount anywhere in `value`.
fn fungible_total<T>(value: &subxt::ext::scale_value::Value<T>) -> u128 {
    match &value.value {
        ValueDef::Variant(variant) if variant.name == "Fungible" => variant
            .values
            .values()
            .next()
            .and_then(|amount| as_u128(amount).ok())
            .unwrap_or(0),
        ValueDef::Variant(variant) => variant.values.values().map(fungible_total).sum(),
        ValueDef::Composite(composite) => composite.values().map(fungible_total).sum(),
        _ => 0,
    }
}

/// Whether any variant named `name` appears anywhere in `value`.
fn mentions_variant<T>(value: &subxt::ext::scale_value::Value<T>, name: &str) -> bool {
    match &value.value {
        ValueDef::Variant(variant) => {
            variant.name == name || variant.values.values().any(|inner| mentions_variant(inner, name))
        }
        ValueDef::Composite(composite) => composite.values().any(|inner| mentions_variant(inner, name)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use frame_metadata::RuntimeMetadataPrefixed;

    const CASH: u32 = 50_000_413;
    const PLACES: Places = Places {
        network: FundingNetwork {
            cash_asset_id: CASH,
        },
        asset_hub_para: 1500,
        people_para: 1502,
        assets_pallet: 50,
    };

    fn asset_hub_metadata() -> subxt::Metadata {
        let bytes = include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-metadata.scale");
        let prefixed = RuntimeMetadataPrefixed::decode(&mut &bytes[..]).expect("fixture decodes");
        subxt::Metadata::try_from(prefixed).expect("fixture converts")
    }

    // The program is built from names, not indices, so a renamed pallet,
    // call, instruction or field fails here instead of on chain.
    #[test]
    fn a_teleport_encodes_as_an_asset_hub_xcm_execute() {
        let metadata = asset_hub_metadata();
        let call = PLACES
            .conversion_call(&[1; 32], 1_000_000, 100_000, Converter::Teleport)
            .expect("sized")
            .runtime_call();
        let pallet = metadata.pallet_by_name("PolkadotXcm").expect("pallet");
        let execute = pallet.call_variant_by_name("execute").expect("call");

        let encoded = call.encode(&metadata).expect("encodes");

        assert_eq!(encoded[..2], [pallet.call_index(), execute.index]);
    }

    // The swaps are XCM `ExchangeAsset`s built from names, as getcash builds
    // them: one hop for the native token, two through it for a stablecoin.
    // Each encodes as an Asset Hub `PolkadotXcm.execute`.
    #[test]
    fn pool_swaps_encode_as_asset_hub_xcm_executes() {
        let metadata = asset_hub_metadata();
        let program = |from, min_native| {
            let call = PLACES
                .conversion_call(
                    &[1; 32],
                    10_000_000_000,
                    100_000_000,
                    Converter::Swap(PoolSwap {
                        from,
                        min_native,
                        min_cash: 1_000_000,
                    }),
                )
                .expect("sized");
            let shape: Vec<_> = call
                .program
                .iter()
                .map(|instruction| {
                    let name = variant_name(instruction).unwrap_or_default().to_string();
                    let exchange = (name == "ExchangeAsset").then(|| {
                        let fields = variant_fields(instruction).expect("fields");
                        let give = fields.values().next().and_then(variant_name).map(str::to_string);
                        let maximal = fields.values().nth(2).map(|maximal| maximal.to_string());
                        (give, maximal)
                    });
                    (name, exchange)
                })
                .collect();
            (call.runtime_call().encode(&metadata).map(|bytes| bytes[..2].to_vec()), shape)
        };
        let pallet = metadata.pallet_by_name("PolkadotXcm").expect("pallet");
        let execute = vec![
            pallet.call_index(),
            pallet.call_variant_by_name("execute").expect("call").index,
        ];
        let step = |name: &str| (name.to_string(), None);
        let exchange = |give: &str| {
            (
                "ExchangeAsset".to_string(),
                Some((Some(give.to_string()), Some("true".to_string()))),
            )
        };
        let tail = [step("InitiateTransfer"), step("RefundSurplus"), step("DepositAsset")];

        assert_eq!(
            [
                program(DepositAsset::Native, None),
                program(DepositAsset::Asset(1984), Some(1_000_000_000)),
            ],
            [
                (
                    Ok(execute.clone()),
                    [vec![step("WithdrawAsset"), step("PayFees"), exchange("Definite")], tail.to_vec()]
                        .concat(),
                ),
                (
                    Ok(execute),
                    [
                        vec![
                            step("WithdrawAsset"),
                            step("PayFees"),
                            exchange("Definite"),
                            exchange("Wild"),
                        ],
                        tail.to_vec(),
                    ]
                    .concat(),
                ),
            ]
        );
    }

    // A swap's least output is its quote less the headroom, and asking the
    // pools for `with_slippage_room` gives an output that, less the headroom,
    // still covers what was wanted. A fee draft keeps a swap able to pay for
    // People whatever its real outputs.
    #[test]
    fn slippage_room_covers_the_least_output_and_drafts_stay_payable() {
        let drafted = Converter::Swap(PoolSwap {
            from: DepositAsset::Asset(1984),
            min_native: Some(77),
            min_cash: 5,
        })
        .drafted();
        let Converter::Swap(drafted) = drafted else {
            panic!("a swap drafts as a swap");
        };

        assert_eq!(
            (
                less_slippage(2_000_000),
                less_slippage(with_slippage_room(2_000_000)) >= 2_000_000,
                (drafted.min_native, drafted.min_cash),
            ),
            (1_900_000, true, (Some(1), 2 * REMOTE_FEE_FLOOR))
        );
    }

    // Only a dry run that ran out of fees is worth retrying with more; the
    // XCM error inside a failed local execution is what says so.
    #[test]
    fn a_failed_execution_names_its_xcm_error() {
        let metadata = asset_hub_metadata();
        let index = metadata.pallet_by_name("PolkadotXcm").expect("pallet").error_index();
        let incomplete = metadata
            .pallet_by_name("PolkadotXcm")
            .and_then(|pallet| pallet.error_variants())
            .and_then(|variants| {
                variants
                    .iter()
                    .find(|variant| variant.name == "LocalExecutionIncompleteWithError")
            })
            .expect("variant")
            .index;
        let execution = Value::unnamed_variant(
            "Err",
            [Value::named_composite([(
                "error",
                Value::named_variant(
                    "Module",
                    [
                        ("index", Value::u128(index.into())),
                        (
                            "error",
                            Value::unnamed_composite(
                                [incomplete, 4, 19, 0].map(|byte| Value::u128(byte.into())),
                            ),
                        ),
                    ],
                ),
            )])],
        );

        assert_eq!(
            module_error_names(&metadata, &execution),
            ["LocalExecutionIncompleteWithError", "NotHoldingFees"]
        );
    }


    // Sized from the getcash formulas: the PSM keeps its fee, rounded up,
    // and every fee estimate gets a tenth more, rounded up.
    #[test]
    fn psm_and_fee_sizing_round_against_the_user() {
        let six_to_six = PsmTerms {
            minting_enabled: true,
            min_swap_amount: 0,
            internal_decimals: 6,
            external_decimals: 6,
            fee_ppm: 5_000,
            headroom: 0,
        };
        let eighteen_to_six = PsmTerms {
            external_decimals: 18,
            ..six_to_six
        };

        assert_eq!(
            (
                psm_mint_out(1_000_000, 5_000),
                psm_mint_out(1_001, 5_000),
                with_margin(1_001),
                six_to_six.to_internal(1_234_567),
                eighteen_to_six.to_internal(1_234_567_000_000_999_999),
            ),
            (995_000, 995, 1_102, 1_234_567, 1_234_567)
        );
    }

    // A quoted deposit must credit at least what the session asked for, or
    // the user is short; each inverse is checked against the forward rule
    // the conversion applies.
    #[test]
    fn sizing_inverts_the_conversion_rounding_up() {
        let terms = PsmTerms {
            minting_enabled: true,
            min_swap_amount: 0,
            internal_decimals: 6,
            external_decimals: 18,
            fee_ppm: 5_000,
            headroom: 0,
        };
        let teleport = |target: u128| {
            let send = teleported_for(target).expect("sized");
            (send, send - remote_fee(send), send - 1 - remote_fee(send - 1))
        };
        let mint = |out: u128| {
            let given = psm_mint_in(out, 5_000).expect("sized");
            (given, psm_mint_out(given, 5_000), psm_mint_out(given - 1, 5_000))
        };

        assert_eq!(
            (
                [teleport(10_000), teleport(2_000_000)],
                [mint(995), mint(1_990_000)],
                (terms.to_external(1), terms.to_internal(terms.to_external(1_234_567))),
                (psm_mint_in(1, 1_000_000), teleported_for(u128::MAX)),
            ),
            (
                [(11_000, 10_000, 9_999), (2_020_202, 2_000_000, 1_999_999)],
                [(1_000, 995, 994), (2_000_000, 1_990_000, 1_989_999)],
                (1_000_000_000_000, 1_234_567),
                (None, None),
            )
        );
    }


    // Without CASH left for execution on People the teleport would land
    // nothing, so a deposit that small is refused before any dry run.
    #[test]
    fn a_deposit_too_small_for_the_fee_on_people_is_refused() {
        let refused = PLACES
            .conversion_call(&[1; 32], 1_500, 600, Converter::Teleport)
            .map(|_| ());

        assert_eq!(
            refused,
            Err(ConversionError::Refused(
                "the deposit does not cover the fee on People".into()
            ))
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod live {
    //! Dry runs against Paseo Next, from accounts that already hold the
    //! assets. Run with `cargo test -p truapi --lib funding::conversion::live -- --ignored`.

    use super::*;

    const ASSET_HUB: &str = "wss://paseo-asset-hub-next-rpc.polkadot.io";
    const PEOPLE: &str = "wss://paseo-people-next-system-rpc.polkadot.io";
    const NETWORK: FundingNetwork = FundingNetwork {
        cash_asset_id: 50_000_413,
    };
    const USDT: u32 = 1984;

    async fn client(url: &str) -> subxt::OnlineClient<SubstrateConfig> {
        let rpc = subxt_rpcs::RpcClient::from_insecure_url(url)
            .await
            .expect("node reachable");
        let backend = subxt::backend::LegacyBackend::builder().build(rpc);
        subxt::OnlineClient::from_backend(std::sync::Arc::new(backend))
            .await
            .expect("client builds")
    }

    async fn chains() -> Chains {
        let asset_hub = client(ASSET_HUB).await;
        let rpc = crate::runtime::statement_allowance::rpc::RpcClient::connect(ASSET_HUB)
            .await
            .expect("node reachable");
        let context = crate::runtime::statement_allowance::ChainContextCache::default()
            .get(&crate::runtime::statement_allowance::ChainClient::new(
                rpc,
                asset_hub.genesis_hash().0,
            ))
            .await
            .expect("metadata");
        Chains::at_finalized(&asset_hub, &client(PEOPLE).await, NETWORK, Some(context))
            .await
            .expect("chains pinned")
    }

    /// An account holding at least `least` of asset `id` on Asset Hub.
    async fn holder(chains: &Chains, id: u32, least: u128) -> [u8; 32] {
        holder_between(chains, id, least, u128::MAX).await
    }

    /// An account holding between `least` and `most` of asset `id`: a
    /// deposit-sized balance, which a swap does not move the pool much for.
    async fn holder_between(chains: &Chains, id: u32, least: u128, most: u128) -> [u8; 32] {
        let mut entries = chains
            .asset_hub
            .storage()
            .iter(
                dynamic::storage::<(Value, Value), Value>("Assets", "Account"),
                (Value::u128(id.into()),),
            )
            .await
            .expect("accounts iterate");
        while let Some(entry) = entries.next().await {
            let entry = entry.expect("entry reads");
            let balance = u128_at(&entry.value().decode().expect("decodes"), "balance").expect("balance");
            if (least..=most).contains(&balance) {
                let key = entry.key_bytes();
                return key[key.len() - 32..].try_into().expect("account id");
            }
        }
        panic!("no account holds {least} of asset {id}");
    }

    fn deposit(asset: DepositAsset, account: [u8; 32], route: ConversionRoute) -> FundingDeposit {
        FundingDeposit {
            source_id: "live".into(),
            number: 1,
            asset,
            account,
            expected: 0,
            route,
            target: None,
            holdings: Vec::new(),
        }
    }

    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn a_cash_deposit_teleports_to_people() {
        let chains = chains().await;
        let account = holder(&chains, NETWORK.cash_asset_id, 5_000_000).await;
        let route = chains
            .choose_route(DepositAsset::Asset(NETWORK.cash_asset_id), 1_000_000)
            .await
            .expect("route");
        let keypair = schnorrkel::MiniSecretKey::from_bytes(&[7; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        let prepared = chains
            .prepare(&deposit(DepositAsset::Asset(NETWORK.cash_asset_id), account, ConversionRoute::Teleport), &keypair, 0)
            .await;
        assert_eq!((route, prepared.map(|_| ())), (Some(ConversionRoute::Teleport), Ok(())));
    }

    // A quote is made before any deposit exists, so its delivery fee is
    // priced on a stand-in for the message the transfer forwards; it must
    // cost what the real one does.
    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn the_quoted_delivery_fee_is_the_real_ones() {
        let chains = chains().await;
        let account = holder(&chains, NETWORK.cash_asset_id, 5_000_000).await;
        let cash = chains.places.cash();
        let call = chains
            .measured(chains.places.conversion_call(&account, 2_000_000, 200_000, Converter::Teleport).expect("sized"))
            .await
            .expect("measured");
        let forwarded = chains.dry_run(&account, &call).await.expect("dry run");

        assert_eq!(
            chains
                .delivery_fee(&chains.places.forwarded_to_people(&account, 1_800_000), &cash)
                .await,
            chains.delivery_fee(&forwarded, &cash).await
        );
    }

    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn deposits_are_quoted_for_cash_and_usdt() {
        let chains = chains().await;
        let quote = |id| chains.deposit_quote(DepositAsset::Asset(id), 2_000_000);
        let (cash, usdt) = (quote(NETWORK.cash_asset_id).await, quote(USDT).await);
        eprintln!("quotes for 2 CASH: {cash:?} {usdt:?}");

        assert!(
            matches!(cash, Ok(Some(DepositQuote { route: ConversionRoute::Teleport, deposit, .. })) if deposit > 2_000_000)
                && matches!(usdt, Ok(Some(DepositQuote { route: ConversionRoute::Psm { .. }, deposit, .. })) if deposit > 2_000_000),
            "{cash:?} {usdt:?}"
        );
    }

    // The dry runs above never see the signed extensions. Signed by an
    // account with nothing to pay with, a well-formed transaction fails on
    // payment alone; a mis-encoded extension or signature fails before that.
    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn a_signed_conversion_is_well_formed_down_to_its_fee_payment() {
        let chains = chains().await;
        let account = holder(&chains, NETWORK.cash_asset_id, 5_000_000).await;
        let keypair = schnorrkel::MiniSecretKey::from_bytes(&[7; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        let extrinsic = chains
            .prepare(
                &deposit(DepositAsset::Asset(NETWORK.cash_asset_id), account, ConversionRoute::Teleport),
                &keypair,
                0,
            )
            .await
            .expect("prepared")
            .extrinsic;
        let validity = chains
            .asset_hub
            .tx()
            .from_bytes(extrinsic)
            .validate()
            .await
            .expect("validates");
        assert_eq!(
            format!("{validity:?}"),
            format!("{:?}", ValidationResult::Invalid(subxt::tx::TransactionInvalid::Payment))
        );
    }

    /// An account holding at least `least` of the native token.
    async fn native_holder(chains: &Chains, least: u128) -> [u8; 32] {
        let mut entries = chains
            .asset_hub
            .storage()
            .iter(dynamic::storage::<(Value,), Value>("System", "Account"), ())
            .await
            .expect("accounts iterate");
        while let Some(entry) = entries.next().await {
            let entry = entry.expect("entry reads");
            let value = entry.value().decode().expect("decodes");
            let free = u128_at(field(&value, "data").expect("data"), "free").expect("free");
            if free >= least {
                let key = entry.key_bytes();
                return key[key.len() - 32..].try_into().expect("account id");
            }
        }
        panic!("no account holds {least} of the native token");
    }

    // The native token has no PSM pair, so it always swaps through the
    // pool; a stablecoin swaps the same way, through the native token, when
    // the PSM cannot serve it.
    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn native_and_stable_deposits_swap_through_the_pools_and_teleport() {
        let chains = chains().await;
        let keypair = schnorrkel::MiniSecretKey::from_bytes(&[7; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        let native_account = native_holder(&chains, 100_000_000_000).await;
        let usdt_account = holder_between(&chains, USDT, 5_000_000, 50_000_000).await;
        let route = chains.choose_route(DepositAsset::Native, 10_000_000_000).await;
        let quote = chains.deposit_quote(DepositAsset::Native, 2_000_000).await;
        eprintln!("native route {route:?}, quote {quote:?}");
        let native = chains
            .prepare(&deposit(DepositAsset::Native, native_account, ConversionRoute::Pool), &keypair, 0)
            .await
            .map(|_| ());
        let stable = chains
            .prepare(&deposit(DepositAsset::Asset(USDT), usdt_account, ConversionRoute::Pool), &keypair, 0)
            .await
            .map(|_| ());

        assert_eq!(
            (route, quote.map(|quote| quote.map(|quote| (quote.asset, quote.route))), native, stable),
            (
                Ok(Some(ConversionRoute::Pool)),
                Ok(Some((DepositAsset::Native, ConversionRoute::Pool))),
                Ok(()),
                Ok(())
            )
        );
    }

    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn a_usdt_deposit_mints_through_the_psm_and_teleports() {
        let chains = chains().await;
        let account = holder(&chains, USDT, 5_000_000).await;
        let route = chains
            .choose_route(DepositAsset::Asset(USDT), 2_000_000)
            .await
            .expect("route");
        let Some(route) = route else {
            panic!("the PSM does not serve USDT");
        };
        let keypair = schnorrkel::MiniSecretKey::from_bytes(&[7; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        let prepared = chains
            .prepare(&deposit(DepositAsset::Asset(USDT), account, route), &keypair, 0)
            .await;
        assert_eq!(prepared.map(|_| ()), Ok(()));
    }
}
