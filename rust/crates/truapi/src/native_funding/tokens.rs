//! The tokens the native route takes on Asset Hub, and the route it declares
//! for them.

use truapi::latest::{FundingDirection, FundingRail};

use super::xcm::Location;
use crate::host_logic::worker_manifest::{FundingConfig, FundingRoute, WorkerManifest};

/// The network the native route's crypto route names: the stack's own.
pub const NATIVE_NETWORK: &str = "polkadot";

/// USDT's id in Asset Hub's `Assets` pallet, on Polkadot and Paseo alike.
const USDT_ASSET_ID: u32 = 1984;
/// USDC's id in Asset Hub's `Assets` pallet, on Polkadot and Paseo alike.
const USDC_ASSET_ID: u32 = 1337;

/// Decimals of the payment asset, which balance units count in.
pub const CASH_DECIMALS: u8 = 6;

/// A token the user can send to a session key on Asset Hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativeToken {
	/// The relay chain's native token, PAS on Paseo.
	Dot,
	/// The payment asset itself, as Polkadot wallets name it.
	DotUsd,
	/// Tether's USD on Asset Hub.
	Usdt,
	/// Circle's USD on Asset Hub.
	Usdc,
}

impl NativeToken {
	/// Every token, in the order the route declares them.
	pub const ALL: [NativeToken; 4] =
		[NativeToken::Dot, NativeToken::DotUsd, NativeToken::Usdt, NativeToken::Usdc];

	/// The symbol a route, an ask and a deposit name it by.
	pub fn symbol(self) -> &'static str {
		match self {
			NativeToken::Dot => "DOT",
			NativeToken::DotUsd => "dotUSD",
			NativeToken::Usdt => "USDT",
			NativeToken::Usdc => "USDC",
		}
	}

	/// The token named `symbol`, in any case.
	pub fn from_symbol(symbol: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|token| token.symbol().eq_ignore_ascii_case(symbol))
	}

	/// Decimals of its smallest unit.
	pub fn decimals(self) -> u8 {
		match self {
			NativeToken::Dot => 10,
			NativeToken::DotUsd | NativeToken::Usdt | NativeToken::Usdc => CASH_DECIMALS,
		}
	}

	/// Its id in Asset Hub's `Assets` pallet, with `cash_asset_id` the
	/// stack's payment asset. `None` for the native token, which
	/// `System.Account` holds.
	pub fn asset_id(self, cash_asset_id: u32) -> Option<u32> {
		match self {
			NativeToken::Dot => None,
			NativeToken::DotUsd => Some(cash_asset_id),
			NativeToken::Usdt => Some(USDT_ASSET_ID),
			NativeToken::Usdc => Some(USDC_ASSET_ID),
		}
	}

	/// Its location as Asset Hub keys it.
	pub fn location(self, cash_asset_id: u32) -> Location {
		match self.asset_id(cash_asset_id) {
			None => Location::relay_native(),
			Some(asset_id) => Location::asset_hub_asset(asset_id),
		}
	}
}

/// The Worker manifest the core goes by for the native route: one crypto
/// route on [`NATIVE_NETWORK`] taking `tokens` in. `None` when no token is
/// served, so the route is not offered.
pub fn native_manifest(tokens: &[NativeToken]) -> Option<WorkerManifest> {
	(!tokens.is_empty()).then(|| WorkerManifest {
		funding: Some(FundingConfig {
			routes: Some(vec![FundingRoute {
				rail: FundingRail::Crypto,
				directions: vec![FundingDirection::In],
				assets: tokens.iter().map(|token| token.symbol().to_string()).collect(),
				networks: Some(vec![NATIVE_NETWORK.to_string()]),
				countries: None,
			}]),
		}),
	})
}
