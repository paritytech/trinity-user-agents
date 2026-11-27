//! The allowances a wallet confirmed on chain, kept in the core database so a
//! restarted host serves their keys without asking the chain again.
//!
//! An allowance key is derived from the wallet, so only the confirmation is
//! kept and no secret is stored. A host without a core database keeps nothing
//! and confirms on chain in every activation.

use super::WalletAccountHolder;
use crate::{
	platform::ProductId,
	runtime::{allowances::AllowanceResource, authority::AuthoritySession},
};

impl WalletAccountHolder {
	/// Whether this wallet confirmed `resource` for `product_id` in `period`.
	///
	/// `period` is the Statement Store allowance period, and `None` for a
	/// Bulletin authorization. A database that cannot be read answers `false`,
	/// so the chain is asked.
	#[cfg(not(target_arch = "wasm32"))]
	pub async fn allowance_is_confirmed(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		resource: AllowanceResource,
		period: Option<u32>,
	) -> bool {
		let Ok(db) = self.services.core_db() else {
			return false;
		};
		let owner = session.public_key;
		let product = product_id.to_string();
		let confirmed = db
			.read(move |conn| {
				Ok(conn
					.prepare(
						"SELECT 1 FROM confirmed_allowance
						 WHERE root_public_key = ?1 AND product_id = ?2 AND resource = ?3
						 AND period IS ?4",
					)?
					.exists(rusqlite::params![owner, product, resource_name(resource), period])?)
			})
			.await;
		confirmed.unwrap_or_else(|error| {
			tracing::warn!(%product_id, %error, "failed to read the confirmed allowance");
			false
		})
	}

	/// Record that this wallet confirmed `resource` for `product_id` in
	/// `period`, replacing what it recorded before.
	///
	/// A failed write is logged: the next activation then asks the chain.
	#[cfg(not(target_arch = "wasm32"))]
	pub async fn record_confirmed_allowance(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		resource: AllowanceResource,
		period: Option<u32>,
	) {
		let Ok(db) = self.services.core_db() else {
			return;
		};
		let owner = session.public_key;
		let product = product_id.to_string();
		let recorded = db
			.write(move |tx| {
				tx.execute(
					"INSERT OR REPLACE INTO confirmed_allowance
					 (root_public_key, product_id, resource, period) VALUES (?1, ?2, ?3, ?4)",
					rusqlite::params![owner, product, resource_name(resource), period],
				)?;
				Ok(())
			})
			.await;
		if let Err(error) = recorded {
			tracing::warn!(%product_id, %error, "failed to record the confirmed allowance");
		}
	}

	/// Stop relying on what the active wallet recorded about `resource` for
	/// `product_id`, so its next request is confirmed on chain.
	#[cfg(not(target_arch = "wasm32"))]
	pub async fn forget_confirmed_allowance(
		&self,
		product_id: &ProductId,
		resource: AllowanceResource,
	) {
		let (Ok(db), Some(session)) = (self.services.core_db(), self.session_state.current())
		else {
			return;
		};
		let product = product_id.to_string();
		let forgotten = db
			.write(move |tx| {
				tx.execute(
					"DELETE FROM confirmed_allowance
					 WHERE root_public_key = ?1 AND product_id = ?2 AND resource = ?3",
					rusqlite::params![session.public_key, product, resource_name(resource)],
				)?;
				Ok(())
			})
			.await;
		if let Err(error) = forgotten {
			tracing::warn!(%product_id, %error, "failed to forget the confirmed allowance");
		}
	}

	/// A browser host has no core database, so nothing is confirmed.
	#[cfg(target_arch = "wasm32")]
	pub async fn allowance_is_confirmed(
		&self,
		_session: &AuthoritySession,
		_product_id: &ProductId,
		_resource: AllowanceResource,
		_period: Option<u32>,
	) -> bool {
		false
	}

	/// A browser host has no core database, so nothing was recorded.
	#[cfg(target_arch = "wasm32")]
	pub async fn forget_confirmed_allowance(
		&self,
		_product_id: &ProductId,
		_resource: AllowanceResource,
	) {
	}

	/// A browser host has no core database, so nothing is recorded.
	#[cfg(target_arch = "wasm32")]
	pub async fn record_confirmed_allowance(
		&self,
		_session: &AuthoritySession,
		_product_id: &ProductId,
		_resource: AllowanceResource,
		_period: Option<u32>,
	) {
	}
}

#[cfg(not(target_arch = "wasm32"))]
fn resource_name(resource: AllowanceResource) -> &'static str {
	match resource {
		AllowanceResource::StatementStore => "statement-store",
		AllowanceResource::Bulletin => "bulletin",
	}
}
