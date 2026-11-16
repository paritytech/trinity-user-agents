//! The XCM v5 programs the native route sends with `PolkadotXcm.execute`,
//! ported from getcash's direct pipeline (`packages/funding/src/funding-program.ts`
//! and `psm-batch.ts` at getcash-community 06916ddd4).
//!
//! Every shape withdraws what the session key deposited on Asset Hub, pays
//! the program's own fees, converts to the stack's payment asset (CASH) where
//! it has to, and sends the CASH to the same key on People with one
//! `InitiateTransfer`. The remote program names the key as the claimer of
//! anything it leaves in People's holding, returns the unspent destination
//! fee and deposits everything on the key. A program that refunds on Asset
//! Hub does so after the transfer, as delivery is charged from the fees
//! register inside `InitiateTransfer`.
//!
//! The types encode by name against the runtime's metadata, so a renamed or
//! reshaped XCM type fails loudly instead of encoding a wrong program.

use core::cmp::Ordering;

use subxt::ext::scale_encode::{self, EncodeAsType, TypeResolver};

/// `xcm::v5::Location`.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Location {
	/// Hops up.
	pub parents: u8,
	/// The path down from there.
	pub interior: Junctions,
}

/// `xcm::v5::Junctions`, in the arities the native route uses.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum Junctions {
	/// No junction.
	Here,
	/// One junction.
	X1([Junction; 1]),
	/// Two junctions.
	X2([Junction; 2]),
	/// Three junctions.
	X3([Junction; 3]),
}

/// `xcm::v5::Junction`, in the variants the native route uses.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum Junction {
	/// A parachain by id.
	Parachain(u32),
	/// A 32-byte account, under `network` when it names one.
	AccountId32 {
		/// The consensus system the account belongs to.
		network: Option<NetworkId>,
		/// The account id.
		id: [u8; 32],
	},
	/// A pallet by index.
	PalletInstance(u8),
	/// An index within a pallet, such as an asset id.
	GeneralIndex(u128),
}

/// `xcm::v5::NetworkId`, in the one variant the native route names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum NetworkId {
	/// Polkadot, which Paseo also names itself.
	Polkadot,
}

impl Junction {
	/// The variant's position in `xcm::v5::Junction`, which its `Ord`
	/// compares before the value.
	fn variant_index(&self) -> u8 {
		match self {
			Junction::Parachain(_) => 0,
			Junction::AccountId32 { .. } => 1,
			Junction::PalletInstance(_) => 4,
			Junction::GeneralIndex(_) => 5,
		}
	}

	/// The value its `Ord` compares once the variants agree.
	fn ordinal(&self) -> (u128, Option<[u8; 32]>) {
		match self {
			Junction::Parachain(id) => (u128::from(*id), None),
			Junction::AccountId32 { id, .. } => (0, Some(*id)),
			Junction::PalletInstance(index) => (u128::from(*index), None),
			Junction::GeneralIndex(index) => (*index, None),
		}
	}
}

impl Junctions {
	fn as_slice(&self) -> &[Junction] {
		match self {
			Junctions::Here => &[],
			Junctions::X1(junctions) => junctions,
			Junctions::X2(junctions) => junctions,
			Junctions::X3(junctions) => junctions,
		}
	}
}

/// The runtime's `Ord` on `Location`: parents, then the interior's arity,
/// then each junction by variant and value. An `Assets` list must be sorted
/// by it, or the runtime refuses to decode the call.
impl Ord for Location {
	fn cmp(&self, other: &Self) -> Ordering {
		let (mine, theirs) = (self.interior.as_slice(), other.interior.as_slice());
		self.parents.cmp(&other.parents).then(mine.len().cmp(&theirs.len())).then_with(|| {
			mine.iter()
				.zip(theirs)
				.map(|(a, b)| {
					a.variant_index().cmp(&b.variant_index()).then(a.ordinal().cmp(&b.ordinal()))
				})
				.find(|ordering| ordering.is_ne())
				.unwrap_or(Ordering::Equal)
		})
	}
}

impl PartialOrd for Location {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Location {
	/// The relay chain's native token, as every parachain keys it.
	pub fn relay_native() -> Self {
		Self { parents: 1, interior: Junctions::Here }
	}

	/// A `pallet-assets` asset as Asset Hub keys it: local, pallet 50.
	pub fn asset_hub_asset(asset_id: u32) -> Self {
		Self {
			parents: 0,
			interior: Junctions::X2([
				Junction::PalletInstance(ASSETS_PALLET),
				Junction::GeneralIndex(u128::from(asset_id)),
			]),
		}
	}

	/// A `pallet-assets` asset of Asset Hub as a sibling such as People keys
	/// it: behind Asset Hub's parachain junction, one hop up.
	pub fn asset_hub_asset_from_sibling(asset_hub_para_id: u32, asset_id: u32) -> Self {
		Self {
			parents: 1,
			interior: Junctions::X3([
				Junction::Parachain(asset_hub_para_id),
				Junction::PalletInstance(ASSETS_PALLET),
				Junction::GeneralIndex(u128::from(asset_id)),
			]),
		}
	}

	/// A sibling parachain, as a destination.
	pub fn sibling(para_id: u32) -> Self {
		Self { parents: 1, interior: Junctions::X1([Junction::Parachain(para_id)]) }
	}

	/// A local account with no network, as a beneficiary.
	pub fn account(id: [u8; 32]) -> Self {
		Self { parents: 0, interior: Junctions::X1([Junction::AccountId32 { network: None, id }]) }
	}

	/// A signed account under the relay network, as `claim_assets` finds a
	/// trap by: a claimer without the network matches no signed origin.
	pub fn claimer(id: [u8; 32]) -> Self {
		Self {
			parents: 0,
			interior: Junctions::X1([Junction::AccountId32 {
				network: Some(NetworkId::Polkadot),
				id,
			}]),
		}
	}
}

/// Asset Hub's `Assets` pallet index, as asset locations name it.
const ASSETS_PALLET: u8 = 50;

/// `xcm::v5::AssetId`.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct AssetId(pub Location);

/// `xcm::v5::Fungibility`, fungible only.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum Fungibility {
	/// An amount.
	Fungible(u128),
}

/// `xcm::v5::WildFungibility`, fungible only.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum WildFungibility {
	/// Any amount.
	Fungible,
}

/// `xcm::v5::Asset`.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Asset {
	/// Which asset.
	pub id: AssetId,
	/// How much.
	pub fun: Fungibility,
}

impl Asset {
	/// `amount` of the asset at `location`.
	pub fn fungible(location: Location, amount: u128) -> Self {
		Self { id: AssetId(location), fun: Fungibility::Fungible(amount) }
	}
}

/// `xcm::v5::Assets`: a list the runtime requires sorted by location.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Assets(Vec<Asset>);

impl Assets {
	/// `assets` in the order the runtime's codec insists on.
	pub fn sorted(mut assets: Vec<Asset>) -> Self {
		assets.sort_by(|a, b| a.id.0.cmp(&b.id.0));
		Self(assets)
	}

	/// One asset.
	pub fn one(asset: Asset) -> Self {
		Self(vec![asset])
	}
}

/// `xcm::v5::WildAsset`, in the variants the native route uses.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum WildAsset {
	/// All of one asset in the holding.
	AllOf {
		/// The asset.
		id: AssetId,
		/// Fungible.
		fun: WildFungibility,
	},
	/// Every asset in the holding, weighed as at most this many.
	AllCounted(u32),
}

/// `xcm::v5::AssetFilter`.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum AssetFilter {
	/// Exactly these.
	Definite(Assets),
	/// A pattern over the holding.
	Wild(WildAsset),
}

impl AssetFilter {
	/// All of the asset at `location` in the holding.
	fn all_of(location: Location) -> Self {
		Self::Wild(WildAsset::AllOf { id: AssetId(location), fun: WildFungibility::Fungible })
	}

	/// The one asset left in the holding, whatever it is.
	fn the_one_left() -> Self {
		Self::Wild(WildAsset::AllCounted(1))
	}
}

/// `xcm::v5::AssetTransferFilter`, in the variants CASH moves by.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum AssetTransferFilter {
	/// Teleported: burnt here, minted there.
	Teleport(AssetFilter),
	/// Reserve-transferred from here, Asset Hub being the reserve.
	ReserveDeposit(AssetFilter),
}

/// `xcm::v5::Hint`.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum Hint {
	/// Who may claim what the program leaves trapped.
	AssetClaimer {
		/// The claimer.
		location: Location,
	},
}

/// `xcm::v5::Weight`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Weight {
	/// Computation time.
	pub ref_time: u64,
	/// Proof size.
	pub proof_size: u64,
}

/// `xcm::v5::Instruction`, in the variants the native route uses.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum Instruction {
	/// Take assets from the origin's account into the holding.
	WithdrawAsset(Assets),
	/// Assets arrived as reserve deposits, on the destination.
	ReserveAssetDeposited(Assets),
	/// Assets arrived by teleport, on the destination.
	ReceiveTeleportedAsset(Assets),
	/// Drop the origin.
	ClearOrigin,
	/// Return what fees did not spend to the holding.
	RefundSurplus,
	/// Move assets from the holding to an account.
	DepositAsset {
		/// Which.
		assets: AssetFilter,
		/// Where.
		beneficiary: Location,
	},
	/// Swap in the holding through the runtime's exchanger.
	ExchangeAsset {
		/// What to give.
		give: AssetFilter,
		/// The least to receive.
		want: Assets,
		/// Give all of `give` and receive as much as it buys.
		maximal: bool,
	},
	/// Name the program's topic.
	SetTopic([u8; 32]),
	/// Move `asset` from the holding into the fees register.
	PayFees {
		/// The allowance.
		asset: Asset,
	},
	/// Send assets and a program to `destination`.
	InitiateTransfer {
		/// Where.
		destination: Location,
		/// What pays for the remote execution.
		remote_fees: Option<AssetTransferFilter>,
		/// Whether the remote program keeps this origin.
		preserve_origin: bool,
		/// What moves.
		assets: Vec<AssetTransferFilter>,
		/// The program that runs there.
		remote_xcm: Xcm,
	},
	/// Hints for the executor.
	SetHints {
		/// The hints.
		hints: Vec<Hint>,
	},
}

/// `xcm::v5::Xcm`.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Xcm(pub Vec<Instruction>);

/// `xcm::VersionedXcm`, at v5.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum VersionedXcm {
	/// XCM v5.
	V5(Xcm),
}

/// `PolkadotXcm.execute` arguments.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ExecuteArgs {
	/// The program.
	pub message: VersionedXcm,
	/// The weight the caller pays for at most.
	pub max_weight: Weight,
}

/// `Psm.mint` arguments.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct PsmMintArgs {
	/// The asset the PSM mints: the payment asset.
	pub internal_asset: Location,
	/// The stable it takes in.
	pub external_asset: Location,
	/// How much of the stable, in its base units.
	pub external_amount: u128,
	/// The most fee the caller accepts, in parts per million: the rate it
	/// was quoted, so a change in between fails the call.
	pub max_fee: u32,
}

/// `Utility.batch_all` arguments over calls encoded already.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct BatchAllArgs {
	/// Each call's encoded bytes, pallet and call index first.
	pub calls: Vec<EncodedCall>,
}

/// A call encoded already, written as it is wherever a `RuntimeCall` goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedCall(pub Vec<u8>);

impl EncodeAsType for EncodedCall {
	fn encode_as_type_to<R: TypeResolver>(
		&self,
		_type_id: R::TypeId,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), scale_encode::Error> {
		out.extend_from_slice(&self.0);
		Ok(())
	}
}

/// How CASH moves between Asset Hub and People on a network, as both
/// runtimes' `TrustedQueryApi` allow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CashTransfer {
	/// Burnt on Asset Hub and minted on People.
	Teleport,
	/// Kept on Asset Hub as the reserve, deposited on People.
	Reserve,
}

impl CashTransfer {
	fn filter(self, filter: AssetFilter) -> AssetTransferFilter {
		match self {
			CashTransfer::Teleport => AssetTransferFilter::Teleport(filter),
			CashTransfer::Reserve => AssetTransferFilter::ReserveDeposit(filter),
		}
	}
}

/// Where a program sends the CASH, and as what it reaches People.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeopleLeg {
	/// People's parachain id.
	pub people_para_id: u32,
	/// The payment asset's id in Asset Hub's `Assets` pallet.
	pub cash_asset_id: u32,
	/// How CASH moves to People.
	pub transfer: CashTransfer,
	/// The session key: the beneficiary on People, and the claimer of what a
	/// failure there traps.
	pub beneficiary: [u8; 32],
	/// The destination fee allowance in CASH. The remote `RefundSurplus`
	/// returns what it does not spend.
	pub remote_fees_cash: u128,
}

impl PeopleLeg {
	fn cash(&self, amount: u128) -> Asset {
		Asset::fungible(Location::asset_hub_asset(self.cash_asset_id), amount)
	}

	/// The `InitiateTransfer` every shape ends its CASH move with: the
	/// holding's CASH to People, the destination fee earmarked, and a remote
	/// program that names the key as the claimer, refunds the unspent fee and
	/// deposits everything on the key.
	fn transfer(&self) -> Instruction {
		let cash = Location::asset_hub_asset(self.cash_asset_id);
		Instruction::InitiateTransfer {
			destination: Location::sibling(self.people_para_id),
			remote_fees: Some(self.transfer.filter(AssetFilter::Definite(Assets::one(
				self.cash(self.remote_fees_cash),
			)))),
			preserve_origin: false,
			assets: vec![self.transfer.filter(AssetFilter::all_of(cash))],
			remote_xcm: Xcm(vec![
				claimer_hint(self.beneficiary),
				Instruction::RefundSurplus,
				deposit_the_one_left(self.beneficiary),
			]),
		}
	}
}

fn claimer_hint(account: [u8; 32]) -> Instruction {
	Instruction::SetHints { hints: vec![Hint::AssetClaimer { location: Location::claimer(account) }] }
}

fn deposit_the_one_left(account: [u8; 32]) -> Instruction {
	Instruction::DepositAsset {
		assets: AssetFilter::the_one_left(),
		beneficiary: Location::account(account),
	}
}

fn execute(instructions: Vec<Instruction>, max_weight: Weight) -> ExecuteArgs {
	ExecuteArgs { message: VersionedXcm::V5(Xcm(instructions)), max_weight }
}

/// The pool tier: native in, CASH on People, every Asset Hub fee paid in
/// native before the exchange. Everything withdrawn above the fee allowance
/// is given to the pool; anything above `min_cash_out` lands as extra CASH.
/// The allowance is exact, as nothing refunds it to an emptied account.
pub fn pool_program(
	leg: &PeopleLeg,
	withdraw_native: u128,
	pay_fees_native: u128,
	min_cash_out: u128,
	max_weight: Weight,
) -> ExecuteArgs {
	let native = |amount| Asset::fungible(Location::relay_native(), amount);
	execute(
		vec![
			Instruction::WithdrawAsset(Assets::one(native(withdraw_native))),
			Instruction::PayFees { asset: native(pay_fees_native) },
			Instruction::ExchangeAsset {
				give: AssetFilter::Definite(Assets::one(native(
					withdraw_native.saturating_sub(pay_fees_native),
				))),
				want: Assets::one(leg.cash(min_cash_out)),
				maximal: true,
			},
			leg.transfer(),
		],
		max_weight,
	)
}

/// The PSM tier's program, run after `Psm.mint` in one `Utility.batch_all`:
/// the CASH the mint paid out sent whole, the fees paid in the stable the
/// mint left on the key, and the unspent allowance back on the key, which
/// the stable's `min_balance` keeps alive. Paying fees in the stable keeps
/// the mint at exactly the target.
pub fn psm_program(
	leg: &PeopleLeg,
	withdraw_cash: u128,
	fees: Asset,
	max_weight: Weight,
) -> ExecuteArgs {
	execute(
		vec![
			Instruction::WithdrawAsset(Assets::sorted(vec![fees.clone(), leg.cash(withdraw_cash)])),
			Instruction::PayFees { asset: fees },
			leg.transfer(),
			Instruction::RefundSurplus,
			deposit_the_one_left(leg.beneficiary),
		],
		max_weight,
	)
}

/// The stable pool tier, for a stable the PSM does not take: two exchanges,
/// one hop each as the exchanger takes one pair, stable to native and all of
/// that native to CASH. Fees are paid in the stable from an allowance the
/// program refunds.
pub fn stable_pool_program(
	leg: &PeopleLeg,
	stable: Location,
	withdraw_stable: u128,
	pay_fees_stable: u128,
	min_native_out: u128,
	min_cash_out: u128,
	max_weight: Weight,
) -> ExecuteArgs {
	let stable = |amount| Asset::fungible(stable.clone(), amount);
	execute(
		vec![
			Instruction::WithdrawAsset(Assets::one(stable(withdraw_stable))),
			Instruction::PayFees { asset: stable(pay_fees_stable) },
			Instruction::ExchangeAsset {
				give: AssetFilter::Definite(Assets::one(stable(
					withdraw_stable.saturating_sub(pay_fees_stable),
				))),
				want: Assets::one(Asset::fungible(Location::relay_native(), min_native_out)),
				maximal: true,
			},
			Instruction::ExchangeAsset {
				give: AssetFilter::all_of(Location::relay_native()),
				want: Assets::one(leg.cash(min_cash_out)),
				maximal: true,
			},
			leg.transfer(),
			Instruction::RefundSurplus,
			deposit_the_one_left(leg.beneficiary),
		],
		max_weight,
	)
}

/// The dotUSD tier: the key holds CASH itself, deposited as dotUSD, so the
/// program moves it with no conversion. Fees are paid in CASH from an
/// allowance the program refunds.
pub fn dot_usd_program(
	leg: &PeopleLeg,
	withdraw_cash: u128,
	pay_fees_cash: u128,
	max_weight: Weight,
) -> ExecuteArgs {
	execute(
		vec![
			Instruction::WithdrawAsset(Assets::one(leg.cash(withdraw_cash))),
			Instruction::PayFees { asset: leg.cash(pay_fees_cash) },
			leg.transfer(),
			Instruction::RefundSurplus,
			deposit_the_one_left(leg.beneficiary),
		],
		max_weight,
	)
}

/// A stand-in for the program People receives for `amount` CASH, keyed as
/// People keys it, for weighing it and pricing its delivery before the real
/// one exists. Mirrors the instructions the runtime forwards.
pub fn forwarded_stand_in(
	leg: &PeopleLeg,
	asset_hub_para_id: u32,
	amount: u128,
) -> VersionedXcm {
	let cash = Asset::fungible(
		Location::asset_hub_asset_from_sibling(asset_hub_para_id, leg.cash_asset_id),
		amount,
	);
	let received = |asset: Asset| match leg.transfer {
		CashTransfer::Teleport => Instruction::ReceiveTeleportedAsset(Assets::one(asset)),
		CashTransfer::Reserve => Instruction::ReserveAssetDeposited(Assets::one(asset)),
	};
	VersionedXcm::V5(Xcm(vec![
		received(cash.clone()),
		Instruction::PayFees { asset: cash.clone() },
		received(cash),
		Instruction::ClearOrigin,
		claimer_hint(leg.beneficiary),
		Instruction::RefundSurplus,
		deposit_the_one_left(leg.beneficiary),
		Instruction::SetTopic([0; 32]),
	]))
}

/// Headroom over a measured fee before it is carried as an allowance, in
/// basis points: well above the 0.6% by which the delivery quote under-reports
/// the charge on a shallow pool, small enough that its refund is dust.
pub const FEE_MARGIN_BPS: u128 = 1_000;

/// `fee` with [`FEE_MARGIN_BPS`] on top, rounded up.
pub fn with_fee_margin(fee: u128) -> u128 {
	fee.saturating_add((fee.saturating_mul(FEE_MARGIN_BPS) + 9_999) / 10_000)
}

/// The destination fee allowance: `remote_fee_buffer`, or 1% of what is
/// sent when that is more, and never zero.
pub fn destination_earmark(cash_out: u128, remote_fee_buffer: u128) -> u128 {
	(cash_out / 100).max(remote_fee_buffer).max(1)
}

#[cfg(test)]
mod tests;
