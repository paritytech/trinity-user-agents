//! [`NativeChain`] over the stack's live Asset Hub and People.
//!
//! Every read is at the finalized head. Fees are read from the runtimes:
//! XCM execution and delivery from `XcmPaymentApi`, the dispatch fee from
//! `TransactionPaymentCallApi` priced into the fee asset by its pool, pool
//! prices from `AssetConversionApi`, and the PSM's state and minting fee from
//! its storage. A conversion is dry-run on Asset Hub before it is signed.

use std::sync::Arc;

use subxt::{
	SubstrateConfig,
	client::OnlineClientAtBlock,
	dynamic::{At, Value},
	ext::{
		frame_decode::extrinsics::encode_call_data,
		scale_encode::{EncodeAsFields, EncodeAsType},
		scale_value::{Composite, ValueDef},
	},
	utils::{AccountId32, H256},
};

use super::{
	chain::{
		ChainError, InPrice, NativeChain, Tier, less_slippage, psm_gross_for, psm_mint_out,
		with_slippage,
	},
	keys::SessionKey,
	tokens::NativeToken,
	xcm::{
		Asset, AssetId, BatchAllArgs, CashTransfer, EncodedCall, ExecuteArgs, Location,
		PeopleLeg, PsmMintArgs, VersionedXcm, Weight, destination_earmark, dot_usd_program,
		forwarded_stand_in, pool_program, psm_program, stable_pool_program, with_fee_margin,
	},
};
use crate::{
	chain::{
		BlockBackend, DispatchOutcome, ExtensionError, ExtrinsicService, HashAndNumber,
		InheritedImplication, TransactionExtension, TxSubmitter, WatchEvent,
	},
	chain_runtime::ChainRuntime,
	stack::StackPayment,
};

/// The weight a program is weighed at before the runtime has weighed it.
const PROBE_WEIGHT: Weight = Weight { ref_time: 8_000_000_000, proof_size: 400_000 };
/// What a signed transaction adds to its call's length: the address, the
/// signature and the extensions, generously.
const SIGNED_OVERHEAD_BYTES: u32 = 160;
/// The XCM version a dry run reports forwarded programs in.
const DRY_RUN_XCM_VERSION: u32 = 5;

/// The native route's chain work over the stack's chains.
pub struct LiveNativeChain {
	chain: ChainRuntime,
	extrinsics: Arc<ExtrinsicService>,
	asset_hub: H256,
	people: H256,
	payment: StackPayment,
}

impl LiveNativeChain {
	/// The chain work on Asset Hub `asset_hub` and People `people`, where
	/// the stack keeps `payment`.
	pub fn new(
		chain: ChainRuntime,
		extrinsics: Arc<ExtrinsicService>,
		asset_hub: [u8; 32],
		people: [u8; 32],
		payment: StackPayment,
	) -> Self {
		Self {
			chain,
			extrinsics,
			asset_hub: H256(asset_hub),
			people: H256(people),
			payment,
		}
	}

	fn cash(&self) -> Location {
		Location::asset_hub_asset(self.payment.asset_id)
	}

	fn cash_on_people_location(&self) -> Location {
		Location::asset_hub_asset_from_sibling(self.payment.asset_hub_para_id, self.payment.asset_id)
	}

	async fn at(&self, genesis: H256) -> Result<OnlineClientAtBlock<SubstrateConfig>, ChainError> {
		self.chain.current_block(genesis.as_bytes()).await.map_err(|error| ChainError::new(error.to_string()))
	}
}

/// Runtime API and storage reads at one block.
struct Reads {
	at: OnlineClientAtBlock<SubstrateConfig>,
}

impl Reads {
	async fn call(
		&self,
		api: &str,
		method: &str,
		args: impl subxt::ext::frame_decode::runtime_apis::IntoEncodableValues,
	) -> Result<Value, ChainError> {
		let payload = subxt::dynamic::runtime_api_call::<_, Value>(api, method, args);
		self.at
			.runtime_apis()
			.call(payload)
			.await
			.map(Value::remove_context)
			.map_err(|error| ChainError::new(format!("{api}.{method}: {error}")))
	}

	async fn storage(
		&self,
		pallet: &str,
		entry: &str,
		keys: impl subxt::ext::frame_decode::storage::IntoEncodableValues
		+ subxt::ext::frame_decode::storage::IntoDecodableValues,
	) -> Result<Option<Value>, ChainError> {
		let address = subxt::dynamic::storage::<_, Value>(pallet, entry);
		let fetched = self
			.at
			.storage()
			.try_fetch(address, keys)
			.await
			.map_err(|error| ChainError::new(format!("{pallet}.{entry}: {error}")))?;
		fetched
			.map(|value| value.decode().map(Value::remove_context))
			.transpose()
			.map_err(|error| ChainError::new(format!("{pallet}.{entry}: {error}")))
	}

	fn encode_call(
		&self,
		pallet: &str,
		call: &str,
		args: &impl EncodeAsFields,
	) -> Result<Vec<u8>, ChainError> {
		let metadata = self.at.metadata_ref();
		encode_call_data(pallet, call, args, metadata, metadata.types())
			.map_err(|error| ChainError::new(format!("{pallet}.{call} does not encode: {error}")))
	}

	/// What the pool between `from` and `to` takes of `from` for exactly
	/// `amount` of `to`, its fee included.
	async fn price_for_exact(
		&self,
		from: &Location,
		to: &Location,
		amount: u128,
	) -> Result<u128, ChainError> {
		let quoted = self
			.call(
				"AssetConversionApi",
				"quote_price_tokens_for_exact_tokens",
				(from, to, amount, true),
			)
			.await?;
		some(quoted).and_then(|value| value.as_u128()).ok_or_else(|| ChainError::new("the pool cannot price it"))
	}

	/// What the pool between `from` and `to` returns of `to` for exactly
	/// `amount` of `from`, its fee included.
	async fn price_of_exact(
		&self,
		from: &Location,
		to: &Location,
		amount: u128,
	) -> Result<u128, ChainError> {
		let quoted = self
			.call(
				"AssetConversionApi",
				"quote_price_exact_tokens_for_tokens",
				(from, to, amount, true),
			)
			.await?;
		some(quoted).and_then(|value| value.as_u128()).ok_or_else(|| ChainError::new("the pool cannot price it"))
	}

	/// Whether the pool between `a` and `b` holds reserves on both sides.
	async fn pool_has_reserves(&self, a: &Location, b: &Location) -> Result<bool, ChainError> {
		let reserves = self.call("AssetConversionApi", "get_reserves", (a, b)).await?;
		Ok(some(reserves).is_some_and(|reserves| {
			let unnamed: Vec<u128> =
				fields(&reserves).filter_map(|value| value.as_u128()).collect();
			unnamed.len() == 2 && unnamed.iter().all(|reserve| *reserve > 0)
		}))
	}

	/// The PSM's minting fee for `external`, when the PSM has an instance
	/// for `internal` and mints against `external` now.
	async fn psm_minting_fee(
		&self,
		internal: &Location,
		external: &Location,
	) -> Result<Option<u32>, ChainError> {
		if self.storage("Psm", "Psm", (internal.clone(),)).await?.is_none() {
			return Ok(None);
		}
		let Some(approval) = self.storage("Psm", "ExternalAssets", (internal.clone(), external.clone())).await?
		else {
			return Ok(None);
		};
		let enabled = approval
			.at("status")
			.and_then(variant_name)
			.is_some_and(|status| status == "AllEnabled");
		if !enabled {
			return Ok(None);
		}
		let fee = self.storage("Psm", "MintingFee", (internal.clone(), external.clone())).await?;
		Ok(Some(fee.and_then(|fee| fee.as_u128()).and_then(|fee| u32::try_from(fee).ok()).unwrap_or(DEFAULT_PSM_FEE)))
	}

	/// The `min_balance` of `asset_id` in Asset Hub's `Assets` pallet.
	async fn min_balance(&self, asset_id: u32) -> Result<u128, ChainError> {
		self.storage("Assets", "Asset", (asset_id,))
			.await?
			.and_then(|details| details.at("min_balance").and_then(|value| value.as_u128()))
			.ok_or_else(|| ChainError::new(format!("asset {asset_id} is not on Asset Hub")))
	}

	/// What executing `program` costs in `fee_asset`: its weight, the local
	/// execution fee, and the fee for delivering `forwarded` to People.
	async fn xcm_fees(
		&self,
		program: &VersionedXcm,
		forwarded: &VersionedXcm,
		people_para_id: u32,
		fee_asset: &Location,
	) -> Result<(Weight, u128), ChainError> {
		let weight = ok(self.call("XcmPaymentApi", "query_xcm_weight", (program,)).await?)?;
		let weight = Weight {
			ref_time: weight.at("ref_time").and_then(|value| value.as_u128()).unwrap_or_default() as u64,
			proof_size: weight.at("proof_size").and_then(|value| value.as_u128()).unwrap_or_default() as u64,
		};
		let asset_id = VersionedAssetId::V5(AssetId(fee_asset.clone()));
		let local = ok(self
			.call("XcmPaymentApi", "query_weight_to_asset_fee", (weight, &asset_id))
			.await?)?
		.as_u128()
		.ok_or_else(|| ChainError::new("the local fee is not an amount"))?;
		let destination = VersionedLocation::V5(Location::sibling(people_para_id));
		let delivered = ok(self
			.call("XcmPaymentApi", "query_delivery_fees", (&destination, forwarded, &asset_id))
			.await?)?;
		let delivery = first_fungible(&delivered).unwrap_or_default();
		Ok((weight, local + delivery))
	}

	/// The dispatch fee of `call`, in the native token.
	async fn dispatch_fee(&self, call: &[u8]) -> Result<u128, ChainError> {
		let length = u32::try_from(call.len()).unwrap_or(u32::MAX).saturating_add(SIGNED_OVERHEAD_BYTES);
		let info = self
			.call("TransactionPaymentCallApi", "query_call_info", (EncodedCall(call.to_vec()), length))
			.await?;
		info.at("partial_fee")
			.and_then(|value| value.as_u128())
			.ok_or_else(|| ChainError::new("the runtime does not price the call"))
	}

	/// Whether `call`, signed by `account`, would dispatch successfully.
	async fn dry_run(&self, account: [u8; 32], call: &[u8]) -> Result<(), ChainError> {
		let origin = OriginCaller::system(RawOrigin::Signed(AccountId32(account)));
		let run = ok(self
			.call(
				"DryRunApi",
				"dry_run_call",
				(origin, EncodedCall(call.to_vec()), DRY_RUN_XCM_VERSION),
			)
			.await?)?;
		let result = run.at("execution_result").ok_or_else(|| ChainError::new("no execution result"))?;
		match variant_name(result) {
			Some("Ok") => Ok(()),
			_ => Err(ChainError::new(format!("Asset Hub would refuse the conversion: {result}"))),
		}
	}
}

/// The PSM's minting fee when its storage holds none, in parts per million.
const DEFAULT_PSM_FEE: u32 = 5_000;

/// `xcm::VersionedAssetId`, at v5.
#[derive(Clone, Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
enum VersionedAssetId {
	V5(AssetId),
}

/// `xcm::VersionedLocation`, at v5.
#[derive(Clone, Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
enum VersionedLocation {
	V5(Location),
}

/// `xcm::VersionedAsset`, at v5.
#[derive(Clone, Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
enum VersionedAsset {
	V5(Asset),
}

/// The runtime's `OriginCaller`, as a signed account.
#[derive(Clone, Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
#[allow(non_camel_case_types)]
enum OriginCaller {
	system(RawOrigin),
}

/// `frame_system::RawOrigin`, signed.
#[derive(Clone, Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
enum RawOrigin {
	Signed(AccountId32),
}

/// `ChargeAssetTxPayment` paying the fee in `asset_id`, with no tip.
struct PayFeeIn {
	asset_id: Location,
}

/// `ChargeAssetTxPayment`'s explicit value.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct AssetTxPayment {
	tip: u128,
	asset_id: Option<Location>,
}

impl TransactionExtension for PayFeeIn {
	type Implicit = ();
	type Explicit = AssetTxPayment;

	fn name(&self) -> &str {
		"ChargeAssetTxPayment"
	}

	fn implicit(&self) {}

	fn explicit(&self, _implication: &InheritedImplication) -> Result<AssetTxPayment, ExtensionError> {
		Ok(AssetTxPayment { tip: 0, asset_id: Some(self.asset_id.clone()) })
	}
}

fn variant_name(value: &Value) -> Option<&str> {
	match &value.value {
		ValueDef::Variant(variant) => Some(variant.name.as_str()),
		_ => None,
	}
}

fn fields(value: &Value) -> impl Iterator<Item = &Value> {
	let composite = match &value.value {
		ValueDef::Variant(variant) => Some(&variant.values),
		ValueDef::Composite(composite) => Some(composite),
		_ => None,
	};
	composite.into_iter().flat_map(Composite::values)
}

/// The value inside `Some`, or a lone field's value.
fn some(value: Value) -> Option<Value> {
	match variant_name(&value) {
		Some("None") => None,
		Some("Some") => fields(&value).next().cloned(),
		_ => Some(value),
	}
}

/// The value inside `Ok`, or the error inside `Err`.
fn ok(value: Value) -> Result<Value, ChainError> {
	match variant_name(&value) {
		Some("Ok") => fields(&value).next().cloned().ok_or_else(|| ChainError::new("an empty Ok")),
		Some("Err") => Err(ChainError::new(format!("the runtime refused: {value}"))),
		_ => Ok(value),
	}
}

/// The amount of the first fungible asset in a `VersionedAssets` value.
fn first_fungible(assets: &Value) -> Option<u128> {
	let list = fields(assets).next()?;
	let first = fields(list).next()?;
	let list_or_asset = if first.at("fun").is_some() { first } else { fields(first).next()? };
	let fun = list_or_asset.at("fun")?;
	fields(fun).next()?.as_u128()
}

/// What a token's conversion needs from the chain before it is built.
struct Plan {
	tier: Tier,
	/// The asset every Asset Hub fee is paid in.
	fee_asset: Location,
	/// What the key keeps of the fee asset to stay alive: its `min_balance`,
	/// or nothing for the native token, whose account the pool tier empties.
	keep: u128,
}

impl LiveNativeChain {
	async fn plan(&self, reads: &Reads, token: NativeToken) -> Result<Option<Plan>, ChainError> {
		let native = Location::relay_native();
		let cash = self.cash();
		let token_location = token.location(self.payment.asset_id);
		let plan = match token {
			NativeToken::Dot => reads
				.pool_has_reserves(&native, &cash)
				.await?
				.then_some(Plan { tier: Tier::Pool, fee_asset: native, keep: 0 }),
			NativeToken::DotUsd => Some(Plan {
				tier: Tier::Direct,
				keep: reads.min_balance(self.payment.asset_id).await?,
				fee_asset: cash,
			}),
			NativeToken::Usdt | NativeToken::Usdc => {
				let asset_id = token.asset_id(self.payment.asset_id).unwrap_or_default();
				let tier = match reads.psm_minting_fee(&cash, &token_location).await? {
					Some(fee_rate) => Some(Tier::Psm { fee_rate }),
					None => (reads.pool_has_reserves(&token_location, &native).await? &&
						reads.pool_has_reserves(&native, &cash).await?)
						.then_some(Tier::StablePool),
				};
				match tier {
					Some(tier) => Some(Plan {
						tier,
						keep: reads.min_balance(asset_id).await?,
						fee_asset: token_location,
					}),
					None => None,
				}
			},
		};
		Ok(plan)
	}

	/// How CASH moves to People: teleported when Asset Hub trusts People to
	/// take it and People agrees or cannot say, reserve-transferred
	/// otherwise.
	async fn cash_transfer(&self, reads: &Reads) -> Result<CashTransfer, ChainError> {
		let cash = VersionedAsset::V5(Asset::fungible(self.cash(), 1));
		let people = VersionedLocation::V5(Location::sibling(self.payment.people_para_id));
		let teleports = ok(reads.call("TrustedQueryApi", "is_trusted_teleporter", (&cash, &people)).await?)?
			.as_bool()
			.unwrap_or(false);
		if !teleports {
			return Ok(CashTransfer::Reserve);
		}
		let people_reads = Reads { at: self.at(self.people).await? };
		let on_people = VersionedAsset::V5(Asset::fungible(self.cash_on_people_location(), 1));
		let asset_hub = VersionedLocation::V5(Location::sibling(self.payment.asset_hub_para_id));
		let agreed = people_reads
			.call("TrustedQueryApi", "is_trusted_teleporter", (&on_people, &asset_hub))
			.await
			.and_then(ok)
			.map_or(true, |answer| answer.as_bool().unwrap_or(true));
		Ok(if agreed { CashTransfer::Teleport } else { CashTransfer::Reserve })
	}

	/// What People keeps of the CASH it receives for executing the program,
	/// with the margin on top: its weight priced in CASH where People can,
	/// a cent otherwise. The unspent part comes back to the key.
	async fn people_fee(&self, leg: &PeopleLeg, amount: u128) -> u128 {
		let priced = async {
			let reads = Reads { at: self.at(self.people).await? };
			let program = forwarded_stand_in(leg, self.payment.asset_hub_para_id, amount);
			let weight = ok(reads.call("XcmPaymentApi", "query_xcm_weight", (&program,)).await?)?;
			let weight = Weight {
				ref_time: weight.at("ref_time").and_then(|value| value.as_u128()).unwrap_or_default() as u64,
				proof_size: weight.at("proof_size").and_then(|value| value.as_u128()).unwrap_or_default()
					as u64,
			};
			let asset = VersionedAssetId::V5(AssetId(self.cash_on_people_location()));
			ok(reads.call("XcmPaymentApi", "query_weight_to_asset_fee", (weight, &asset)).await?)?
				.as_u128()
				.ok_or_else(|| ChainError::new("People's fee is not an amount"))
		};
		with_fee_margin(priced.await.unwrap_or(super::chain::CREDIT_UNIT))
	}

	fn leg(&self, transfer: CashTransfer, beneficiary: [u8; 32], cash_out: u128, people_fee: u128) -> PeopleLeg {
		PeopleLeg {
			people_para_id: self.payment.people_para_id,
			cash_asset_id: self.payment.asset_id,
			transfer,
			beneficiary,
			remote_fees_cash: destination_earmark(cash_out, people_fee),
		}
	}

	/// The `PolkadotXcm.execute` or PSM batch that converts `deposit` of a
	/// token through `plan`, with its fee allowance `fees` and dispatch fee
	/// `dispatch`, both in the fee asset. `None` when the deposit does not
	/// cover them.
	#[allow(clippy::too_many_arguments)]
	async fn conversion_call(
		&self,
		reads: &Reads,
		plan: &Plan,
		leg: &PeopleLeg,
		deposit: u128,
		fees: u128,
		dispatch: u128,
		weight: Weight,
	) -> Result<Option<ConversionCall>, ChainError> {
		let native = Location::relay_native();
		let cash = self.cash();
		let Some(spend) = deposit.checked_sub(dispatch + plan.keep + fees).filter(|spend| *spend > 0)
		else {
			return Ok(None);
		};
		let execute = |args: ExecuteArgs| {
			let encoded = reads.encode_call("PolkadotXcm", "execute", &args)?;
			Ok::<_, ChainError>(ConversionCall::Execute(args, encoded))
		};
		let call = match plan.tier {
			Tier::Pool => {
				let cash_out = reads.price_of_exact(&native, &cash, spend).await?;
				execute(pool_program(leg, spend + fees, fees, less_slippage(cash_out), weight))?
			},
			Tier::Direct => execute(dot_usd_program(leg, spend + fees, fees, weight))?,
			Tier::StablePool => {
				let native_out = less_slippage(reads.price_of_exact(&plan.fee_asset, &native, spend).await?);
				let cash_out = less_slippage(reads.price_of_exact(&native, &cash, native_out).await?);
				execute(stable_pool_program(
					leg,
					plan.fee_asset.clone(),
					spend + fees,
					fees,
					native_out,
					cash_out,
					weight,
				))?
			},
			Tier::Psm { fee_rate } => {
				let minted = psm_mint_out(spend, fee_rate);
				let mint = PsmMintArgs {
					internal_asset: cash,
					external_asset: plan.fee_asset.clone(),
					external_amount: spend,
					max_fee: fee_rate,
				};
				let fee_allowance = Asset::fungible(plan.fee_asset.clone(), fees);
				let program = reads.encode_call(
					"PolkadotXcm",
					"execute",
					&psm_program(leg, minted, fee_allowance, weight),
				)?;
				let mint = reads.encode_call("Psm", "mint", &mint)?;
				let batch = BatchAllArgs { calls: vec![EncodedCall(mint), EncodedCall(program)] };
				let encoded = reads.encode_call("Utility", "batch_all", &batch)?;
				ConversionCall::Batch(batch, encoded)
			},
		};
		Ok(Some(call))
	}

	/// The XCM program of a call `conversion_call` built, for weighing.
	fn program_of(plan: &Plan, leg: &PeopleLeg, deposit: u128, fees: u128) -> VersionedXcm {
		let probe = |amount: u128| amount.max(2);
		let spend = probe(deposit / 2);
		let args = match plan.tier {
			Tier::Pool => pool_program(leg, spend + fees, fees, 1, PROBE_WEIGHT),
			Tier::Direct => dot_usd_program(leg, spend + fees, fees, PROBE_WEIGHT),
			Tier::StablePool => stable_pool_program(
				leg,
				plan.fee_asset.clone(),
				spend + fees,
				fees,
				1,
				1,
				PROBE_WEIGHT,
			),
			Tier::Psm { .. } => psm_program(
				leg,
				spend,
				Asset::fungible(plan.fee_asset.clone(), fees),
				PROBE_WEIGHT,
			),
		};
		args.message
	}

	/// Every fee of converting `deposit` through `plan`, in the fee asset:
	/// the XCM allowance, the dispatch fee, and the weight to declare.
	async fn fees(
		&self,
		reads: &Reads,
		plan: &Plan,
		leg: &PeopleLeg,
		deposit: u128,
	) -> Result<(u128, u128, Weight), ChainError> {
		let rough = deposit / 8;
		let program = Self::program_of(plan, leg, deposit, rough);
		let forwarded = forwarded_stand_in(leg, self.payment.asset_hub_para_id, deposit);
		let (weight, xcm_fee) =
			reads.xcm_fees(&program, &forwarded, self.payment.people_para_id, &plan.fee_asset).await?;
		// The pool tier's allowance is exact: nothing refunds it to an
		// emptied account. The others refund what they do not spend.
		let fees = match plan.tier {
			Tier::Pool => xcm_fee,
			_ => with_fee_margin(xcm_fee),
		};
		let call = self
			.conversion_call(reads, plan, leg, deposit, fees, 0, weight)
			.await?
			.ok_or_else(|| ChainError::new("the deposit does not cover the program's fees"))?;
		let dispatch_native = with_fee_margin(reads.dispatch_fee(call.encoded()).await?);
		let dispatch = if plan.fee_asset == Location::relay_native() {
			dispatch_native
		} else {
			reads.price_for_exact(&plan.fee_asset, &Location::relay_native(), dispatch_native).await?
		};
		Ok((fees, dispatch, weight))
	}
}

#[async_trait::async_trait]
impl NativeChain for LiveNativeChain {
	async fn tokens_in(&self) -> Result<Vec<NativeToken>, ChainError> {
		let reads = Reads { at: self.at(self.asset_hub).await? };
		self.cash_transfer(&reads).await?;
		let mut tokens = Vec::new();
		for token in NativeToken::ALL {
			if self.plan(&reads, token).await?.is_some() {
				tokens.push(token);
			}
		}
		Ok(tokens)
	}

	async fn price_in(&self, token: NativeToken, cash: u128) -> Result<InPrice, ChainError> {
		let reads = Reads { at: self.at(self.asset_hub).await? };
		let plan = self
			.plan(&reads, token)
			.await?
			.ok_or_else(|| ChainError::new(format!("{} is not served", token.symbol())))?;
		let transfer = self.cash_transfer(&reads).await?;
		let probe_leg = self.leg(transfer, [0; 32], cash, 0);
		let people_fee = self.people_fee(&probe_leg, cash).await;
		let landing = cash + people_fee;
		let leg = self.leg(transfer, [0; 32], landing, people_fee);
		let native = Location::relay_native();
		// What the conversion itself takes in the token for `landing` CASH.
		let converted = match plan.tier {
			Tier::Direct => landing,
			Tier::Pool => with_slippage(reads.price_for_exact(&native, &self.cash(), landing).await?),
			Tier::Psm { fee_rate } => {
				psm_gross_for(landing, fee_rate).ok_or_else(|| ChainError::new("the PSM keeps it all"))?
			},
			Tier::StablePool => {
				let native_in = with_slippage(reads.price_for_exact(&native, &self.cash(), landing).await?);
				with_slippage(reads.price_for_exact(&plan.fee_asset, &native, native_in).await?)
			},
		};
		let (fees, dispatch, _) = self.fees(&reads, &plan, &leg, converted * 2).await?;
		let send = converted + fees + dispatch + plan.keep;
		Ok(InPrice {
			tier: plan.tier,
			send,
			network_fee: send.saturating_sub(cash),
			exact: matches!(plan.tier, Tier::Direct | Tier::Psm { .. }),
		})
	}

	async fn balance(&self, token: NativeToken, account: [u8; 32]) -> Result<u128, ChainError> {
		let reads = Reads { at: self.at(self.asset_hub).await? };
		let held = match token.asset_id(self.payment.asset_id) {
			None => reads
				.storage("System", "Account", (AccountId32(account),))
				.await?
				.and_then(|account| account.at("data").and_then(|data| data.at("free")).and_then(|free| free.as_u128())),
			Some(asset_id) => reads
				.storage("Assets", "Account", (asset_id, AccountId32(account)))
				.await?
				.and_then(|holding| holding.at("balance").and_then(|balance| balance.as_u128())),
		};
		Ok(held.unwrap_or_default())
	}

	async fn nonce(&self, account: [u8; 32]) -> Result<u64, ChainError> {
		let reads = Reads { at: self.at(self.asset_hub).await? };
		let nonce = reads
			.storage("System", "Account", (AccountId32(account),))
			.await?
			.and_then(|account| account.at("nonce").and_then(|nonce| nonce.as_u128()));
		Ok(nonce.map_or(0, |nonce| nonce as u64))
	}

	async fn cash_on_people(&self, account: [u8; 32]) -> Result<u128, ChainError> {
		let reads = Reads { at: self.at(self.people).await? };
		let held = reads
			.storage("ForeignAssets", "Account", (self.cash_on_people_location(), AccountId32(account)))
			.await?
			.and_then(|holding| holding.at("balance").and_then(|balance| balance.as_u128()));
		Ok(held.unwrap_or_default())
	}

	async fn convert(
		&self,
		key: &SessionKey,
		token: NativeToken,
		tier: Tier,
		deposit: u128,
		min_cash: u128,
	) -> Result<(), ChainError> {
		let reads = Reads { at: self.at(self.asset_hub).await? };
		let mut plan = self
			.plan(&reads, token)
			.await?
			.ok_or_else(|| ChainError::new(format!("{} is no longer served", token.symbol())))?;
		// The quoted tier stands, PSM fee rate included, so a change in
		// between fails the call rather than costing more.
		plan.tier = tier;
		let transfer = self.cash_transfer(&reads).await?;
		let probe_leg = self.leg(transfer, key.public, min_cash, 0);
		let people_fee = self.people_fee(&probe_leg, min_cash).await;
		let leg = self.leg(transfer, key.public, min_cash, people_fee);
		let (fees, dispatch, weight) = self.fees(&reads, &plan, &leg, deposit).await?;
		let call = self
			.conversion_call(&reads, &plan, &leg, deposit, fees, dispatch, weight)
			.await?
			.ok_or_else(|| ChainError::new("the deposit does not cover the conversion's fees"))?;
		reads.dry_run(key.public, call.encoded()).await?;

		let builder = self.extrinsics.build_mortal_extrinsic(self.asset_hub);
		let builder = match call {
			ConversionCall::Execute(args, _) => builder.with_call_args("PolkadotXcm", "execute", args),
			ConversionCall::Batch(batch, _) => builder.with_call_args("Utility", "batch_all", batch),
		};
		let mut builder = builder.with_nonce(0).with_signer(&key.signer);
		if plan.fee_asset != Location::relay_native() {
			builder = builder.with_extension(PayFeeIn { asset_id: plan.fee_asset.clone() });
		}
		let extrinsic = builder
			.build()
			.await
			.map_err(|error| ChainError::new(format!("the conversion does not build: {error}")))?
			.extrinsic;
		let mut events = self
			.chain
			.submit_and_watch(self.asset_hub, &extrinsic)
			.await
			.map_err(|error| ChainError::new(error.to_string()))?;
		use futures::StreamExt;
		while let Some(event) = events.next().await {
			match event {
				WatchEvent::InFinalizedBlock(hash) => {
					let number = self
						.chain
						.block_number(self.asset_hub, hash)
						.await
						.map_err(|error| ChainError::new(error.to_string()))?
						.ok_or_else(|| ChainError::new("the finalized block is unknown"))?;
					let outcome = self
						.chain
						.dispatch_outcome(self.asset_hub, HashAndNumber { hash, number }, extrinsic.hash())
						.await
						.map_err(|error| ChainError::new(error.to_string()))?;
					return match outcome {
						Some(DispatchOutcome::Succeeded) => Ok(()),
						Some(DispatchOutcome::Failed(error)) => {
							Err(ChainError::new(format!("the conversion failed: {error}")))
						},
						None => Err(ChainError::new("the finalized block lacks the conversion")),
					};
				},
				event if event.is_terminal() => {
					return Err(ChainError::new(format!("the conversion was not included: {event:?}")));
				},
				_ => {},
			}
		}
		Err(ChainError::new("the conversion's watch ended"))
	}
}

/// A conversion's call, typed for the extrinsic builder, with its encoded
/// bytes for pricing and the dry run.
enum ConversionCall {
	/// `PolkadotXcm.execute`.
	Execute(ExecuteArgs, Vec<u8>),
	/// `Utility.batch_all`, for the PSM's mint before the program.
	Batch(BatchAllArgs, Vec<u8>),
}

impl ConversionCall {
	fn encoded(&self) -> &[u8] {
		match self {
			ConversionCall::Execute(_, encoded) | ConversionCall::Batch(_, encoded) => encoded,
		}
	}
}
