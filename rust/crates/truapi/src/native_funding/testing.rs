//! Fakes the native route's tests share: a chain the test drives, and keys
//! and an installation fixed for the run.

use std::sync::Mutex;

use truapi::latest::GenericError;

use super::{
	chain::{ChainError, InPrice, NativeChain, Tier},
	keys::{SessionKey, SessionKeySlot, SessionKeys, session_key_from_entropy},
	tokens::NativeToken,
};
use crate::{
	installation::{InstallationId, InstallationIdProvider},
	runtime::AuthorityError,
};

/// The entropy [`FixedKeys`] derives from.
pub const ENTROPY: [u8; 16] = [3; 16];
/// What the fake chain asks for a DOT deposit.
pub const SEND: u128 = 25_000_000_000;

/// A chain whose state the test sets, and whose conversion lands `landing`
/// CASH on People.
#[derive(Default)]
pub struct FakeChain {
	state: Mutex<ChainState>,
}

/// What the fake chain holds.
#[derive(Default)]
pub struct ChainState {
	/// What the session key holds on Asset Hub.
	pub deposit: u128,
	/// The key's nonce on Asset Hub.
	pub nonce: u64,
	/// What the key holds of CASH on People.
	pub on_people: u128,
	/// What a conversion lands on People.
	pub landing: u128,
	/// Every conversion it ran.
	pub converted: Vec<(NativeToken, Tier, u128, u128)>,
}

impl FakeChain {
	/// Edit the chain's state.
	pub fn with(&self, edit: impl FnOnce(&mut ChainState)) {
		edit(&mut self.state.lock().unwrap());
	}

	/// Every conversion it ran: token, tier, deposit and least CASH.
	pub fn converted(&self) -> Vec<(NativeToken, Tier, u128, u128)> {
		self.state.lock().unwrap().converted.clone()
	}
}

#[async_trait::async_trait]
impl NativeChain for FakeChain {
	async fn tokens_in(&self) -> Result<Vec<NativeToken>, ChainError> {
		Ok(vec![NativeToken::Dot, NativeToken::DotUsd])
	}

	async fn price_in(&self, token: NativeToken, cash: u128) -> Result<InPrice, ChainError> {
		Ok(match token {
			NativeToken::Dot => InPrice { tier: Tier::Pool, send: SEND, network_fee: 1_000, exact: false },
			_ => InPrice { tier: Tier::Direct, send: cash + 50_000, network_fee: 50_000, exact: true },
		})
	}

	async fn balance(&self, _token: NativeToken, _account: [u8; 32]) -> Result<u128, ChainError> {
		Ok(self.state.lock().unwrap().deposit)
	}

	async fn nonce(&self, _account: [u8; 32]) -> Result<u64, ChainError> {
		Ok(self.state.lock().unwrap().nonce)
	}

	async fn cash_on_people(&self, _account: [u8; 32]) -> Result<u128, ChainError> {
		Ok(self.state.lock().unwrap().on_people)
	}

	async fn convert(
		&self,
		_key: &SessionKey,
		token: NativeToken,
		tier: Tier,
		deposit: u128,
		min_cash: u128,
	) -> Result<(), ChainError> {
		let mut state = self.state.lock().unwrap();
		state.converted.push((token, tier, deposit, min_cash));
		state.deposit = 0;
		state.nonce = 1;
		state.on_people = state.landing;
		Ok(())
	}
}

/// Derives from [`ENTROPY`].
pub struct FixedKeys;

impl SessionKeys for FixedKeys {
	fn session_key(&self, slot: &SessionKeySlot) -> Result<SessionKey, AuthorityError> {
		session_key_from_entropy(&ENTROPY, slot)
			.map_err(|error| AuthorityError::Unavailable { reason: error.to_string() })
	}
}

/// Installation `[9; 32]`.
pub struct FixedInstallation;

#[async_trait::async_trait]
impl InstallationIdProvider for FixedInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		Ok(InstallationId([9; 32]))
	}
}

