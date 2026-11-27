//! The allowances a wallet confirmed on chain, kept in the core database so a
//! restarted host serves their keys without asking the chain again.
//!
//! An allowance key is derived from the wallet, so only the confirmation is
//! kept and no secret is stored. A host without a core database keeps nothing
//! and confirms on chain in every activation.

use rusqlite::{Connection, Transaction, named_params};

use super::WalletAccountHolder;
use crate::{
	platform::ProductId,
	runtime::{allowances::AllowanceResource, authority::AuthoritySession},
	store::DbError,
};

const SELECT_CONFIRMED: &str = "SELECT 1 FROM confirmed_allowance
	WHERE root_public_key = :owner AND product_id = :product AND resource = :resource
	AND period IS :period";

/// Whether the wallet `owner` confirmed `resource` for `product` in `period`.
fn is_confirmed(
	conn: &Connection,
	owner: [u8; 32],
	product: &str,
	resource: AllowanceResource,
	period: Option<u32>,
) -> Result<bool, DbError> {
	Ok(conn.prepare_cached(SELECT_CONFIRMED)?.exists(named_params! {
		":owner": owner,
		":product": product,
		":resource": resource_name(resource),
		":period": period,
	})?)
}

const UPSERT_CONFIRMED: &str = "INSERT OR REPLACE INTO confirmed_allowance
	(root_public_key, product_id, resource, period)
	VALUES (:owner, :product, :resource, :period)";

/// Records that the wallet `owner` confirmed `resource` for `product` in
/// `period`, replacing what it recorded before.
fn record(
	tx: &Transaction<'_>,
	owner: [u8; 32],
	product: &str,
	resource: AllowanceResource,
	period: Option<u32>,
) -> Result<(), DbError> {
	tx.prepare_cached(UPSERT_CONFIRMED)?.execute(named_params! {
		":owner": owner,
		":product": product,
		":resource": resource_name(resource),
		":period": period,
	})?;
	Ok(())
}

const DELETE_CONFIRMED: &str = "DELETE FROM confirmed_allowance
	WHERE root_public_key = :owner AND product_id = :product AND resource = :resource";

/// Removes what the wallet `owner` recorded about `resource` for `product`.
fn forget(
	tx: &Transaction<'_>,
	owner: [u8; 32],
	product: &str,
	resource: AllowanceResource,
) -> Result<(), DbError> {
	tx.prepare_cached(DELETE_CONFIRMED)?.execute(named_params! {
		":owner": owner,
		":product": product,
		":resource": resource_name(resource),
	})?;
	Ok(())
}

fn resource_name(resource: AllowanceResource) -> &'static str {
	match resource {
		AllowanceResource::StatementStore => "statement-store",
		AllowanceResource::Bulletin => "bulletin",
	}
}

impl WalletAccountHolder {
	/// Whether this wallet confirmed `resource` for `product_id` in `period`.
	///
	/// `period` is the Statement Store allowance period, and `None` for a
	/// Bulletin authorization. A database that cannot be read answers `false`,
	/// so the chain is asked.
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
		let confirmed =
			db.read(move |conn| is_confirmed(conn, owner, &product, resource, period)).await;
		confirmed.unwrap_or_else(|error| {
			tracing::warn!(%product_id, %error, "failed to read the confirmed allowance");
			false
		})
	}

	/// Record that this wallet confirmed `resource` for `product_id` in
	/// `period`, replacing what it recorded before.
	///
	/// A failed write is logged: the next activation then asks the chain.
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
		let recorded = db.write(move |tx| record(tx, owner, &product, resource, period)).await;
		if let Err(error) = recorded {
			tracing::warn!(%product_id, %error, "failed to record the confirmed allowance");
		}
	}

	/// Stop relying on what the active wallet recorded about `resource` for
	/// `product_id`, so its next request is confirmed on chain.
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
		let forgotten =
			db.write(move |tx| forget(tx, session.public_key, &product, resource)).await;
		if let Err(error) = forgotten {
			tracing::warn!(%product_id, %error, "failed to forget the confirmed allowance");
		}
	}
}
