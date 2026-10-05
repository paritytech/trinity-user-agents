//! Turning a funding deposit into CASH on People, the way getcash does it.
//!
//! One Asset Hub transaction, signed by the deposit account, converts the
//! deposit (nothing to do for CASH, a PSM mint for an approved stablecoin) and
//! teleports the CASH to the same account on People. Every fee is paid in the
//! deposited asset, since that is all the account holds. Before submitting,
//! the transaction is dry-run on Asset Hub and the message it forwards is
//! dry-run on People, so a conversion that would trap funds is never sent.

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
use std::sync::Arc;

use super::DepositBalances;
use crate::host_logic::funding::{ConversionRoute, DepositAsset, FundingDeposit};
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
    extensions: Arc<ExtensionMetadata>,
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
        extensions: Arc<ExtensionMetadata>,
    ) -> Result<Self, ConversionError> {
        let asset_hub = asset_hub.at_current_block().await.map_err(chain)?;
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
        let DepositAsset::Asset(id) = asset else {
            return Ok(None);
        };
        if id == self.places.network.cash_asset_id {
            return Ok(Some(ConversionRoute::Teleport));
        }
        let Some(psm) = self.psm(id).await? else {
            return Ok(None);
        };
        let amount = psm.to_internal(expected);
        let margin = (amount * PSM_CAPACITY_MARGIN_PERCENT / 100).max(PSM_CAPACITY_MARGIN_FLOOR);
        let serves = psm.minting_enabled
            && amount >= psm.min_swap_amount
            && psm.headroom >= amount.saturating_add(margin);
        Ok(serves.then_some(ConversionRoute::Psm {
            fee_ppm: psm.fee_ppm,
        }))
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
        let fee_ppm = fetch_value(
            &self.asset_hub,
            "Psm",
            "MintingFee",
            vec![cash.clone(), external.clone()],
        )
        .await?
        .map(|fee| as_u128(&fee))
        .transpose()?
        .unwrap_or(0);

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
        let mint = match (deposit.route, deposit.asset) {
            (ConversionRoute::Teleport, _) => None,
            (ConversionRoute::Psm { fee_ppm }, DepositAsset::Asset(id)) => Some(PsmMint {
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
        };

        // A first draft with a generous fee allowance measures the fees,
        // which barely depend on the amounts; the final call is then sized
        // from them.
        let draft = self
            .measured(self.places.conversion_call(&account, spendable, spendable / 5, mint)?)
            .await?;
        let forwarded = self.dry_run(&account, &draft).await?;
        let local_fee = self.local_fee(&draft.program, &fee_asset).await?;
        let delivery_fee = self.delivery_fee(&forwarded, &fee_asset).await?;
        let draft_extrinsic = self.sign(&self.extensions, &signer, &draft, &fee_asset, nonce)?;
        let dispatch_fee = self.dispatch_fee(&draft_extrinsic, &fee_asset).await?;

        let allowance = with_margin(local_fee.saturating_add(delivery_fee));
        let available = spendable
            .checked_sub(with_margin(dispatch_fee))
            .and_then(|left| left.checked_sub(allowance))
            .filter(|left| *left > 0)
            .ok_or_else(|| ConversionError::Refused("the deposit does not cover the fees".into()))?;
        let call = self
            .measured(self.places.conversion_call(&account, available + allowance, allowance, mint)?)
            .await?;
        let forwarded = self.dry_run(&account, &call).await?;
        self.dry_run_on_people(&forwarded).await?;
        Ok(Prepared {
            extrinsic: self.sign(&self.extensions, &signer, &call, &fee_asset, nonce)?,
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
            return Err(ConversionError::Refused(format!(
                "dry run failed on Asset Hub: {execution}"
            )));
        }
        let events = field(&effects, "emitted_events")?;
        if mentions_variant(events, "AssetsTrapped") {
            return Err(ConversionError::Refused("the conversion would trap assets on Asset Hub".into()));
        }
        let forwarded = field(&effects, "forwarded_xcms")?;
        items(forwarded)
            .into_iter()
            .find_map(|entry| {
                let [destination, messages] = items(entry)[..] else {
                    return None;
                };
                (parachain_of(destination) == Some(self.places.people_para))
                    .then(|| items(messages).first().map(|message| (*message).clone()))
                    .flatten()
            })
            .ok_or_else(|| ConversionError::Refused("the conversion forwards nothing to People".into()))
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
        let native = u128_at(&info, "partial_fee")?;
        if fee_asset == &location(1, Vec::new()) {
            return Ok(native);
        }
        let quoted = call_api(
            &self.asset_hub,
            "AssetConversionApi",
            "quote_price_tokens_for_exact_tokens",
            vec![fee_asset.clone(), location(1, Vec::new()), Value::u128(native), Value::bool(true)],
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
            DepositAsset::Native => location(1, Vec::new()),
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
        mint: Option<PsmMint>,
    ) -> Result<ConversionCall, ConversionError> {
        let converted = withdrawn
            .checked_sub(allowance)
            .ok_or_else(|| ConversionError::Refused("the fee allowance exceeds the deposit".into()))?;
        let (cash, withdrawn_assets) = match mint {
            None => (converted, vec![self.asset(&self.cash(), withdrawn)]),
            Some(mint) => {
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
        let remote_fee = (cash * REMOTE_FEE_PERCENT / 100).max(REMOTE_FEE_FLOOR);
        if remote_fee >= cash {
            return Err(ConversionError::Refused(
                "the deposit does not cover the fee on People".into(),
            ));
        }
        let beneficiary = location(
            0,
            vec![Value::named_variant(
                "AccountId32",
                [
                    ("network", Value::unnamed_variant("None", [])),
                    ("id", Value::from_bytes(account)),
                ],
            )],
        );
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
        let fee_asset = match mint {
            None => self.asset(&self.cash(), allowance),
            Some(mint) => self.asset(&self.asset_location(mint.id), allowance),
        };
        let program = vec![
            Value::unnamed_variant("WithdrawAsset", [Value::unnamed_composite(withdrawn_assets)]),
            Value::named_variant("PayFees", [("asset", fee_asset)]),
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
        ];
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

    fn asset(&self, id: &Value, amount: u128) -> Value {
        Value::named_composite([
            ("id", id.clone()),
            ("fun", Value::unnamed_variant("Fungible", [Value::u128(amount)])),
        ])
    }
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
            self.asset_hub_balance(asset, account)
                .await
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
        Value::named_composite([
            ("tip", Value::u128(0)),
            ("asset_id", Value::unnamed_variant("Some", [fee_asset.clone()])),
        ])
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
            .conversion_call(&[1; 32], 1_000_000, 100_000, None)
            .expect("sized")
            .runtime_call();
        let pallet = metadata.pallet_by_name("PolkadotXcm").expect("pallet");
        let execute = pallet.call_variant_by_name("execute").expect("call");

        let encoded = call.encode(&metadata).expect("encodes");

        assert_eq!(encoded[..2], [pallet.call_index(), execute.index]);
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

    // Without CASH left for execution on People the teleport would land
    // nothing, so a deposit that small is refused before any dry run.
    #[test]
    fn a_deposit_too_small_for_the_fee_on_people_is_refused() {
        let refused = PLACES
            .conversion_call(&[1; 32], 1_500, 600, None)
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
        let rpc = crate::runtime::statement_allowance::rpc::RpcClient::connect(ASSET_HUB)
            .await
            .expect("node reachable");
        let extensions = crate::runtime::statement_allowance::fetch_metadata(&rpc)
            .await
            .expect("metadata");
        Chains::at_finalized(
            &client(ASSET_HUB).await,
            &client(PEOPLE).await,
            NETWORK,
            Arc::new(extensions),
        )
        .await
        .expect("chains pinned")
    }

    /// An account holding at least `least` of asset `id` on Asset Hub.
    async fn holder(chains: &Chains, id: u32, least: u128) -> [u8; 32] {
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
            if balance >= least {
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
