//! Queries over `confirmed_allowance`, the allowances a wallet confirmed on
//! chain.
//!
//! An allowance key is derived from the wallet, so only the confirmation is
//! kept and no secret is stored. A host without a core database keeps nothing.

use rusqlite::named_params;

use crate::{
	platform::ProductId,
	runtime::{allowances::AllowanceResource, services::RuntimeServices},
	store::DbError,
};

const SELECT_CONFIRMED: &str = "SELECT 1 FROM confirmed_allowance
	WHERE product_id = :product AND resource = :resource AND period IS :period";

/// Whether the wallet confirmed `resource` for `product_id` in `period`.
///
/// `period` is the Statement Store allowance period, and `None` for a
/// Bulletin authorization.
pub async fn is_confirmed(
	services: &RuntimeServices,
	product_id: &ProductId,
	resource: AllowanceResource,
	period: Option<u32>,
) -> Result<bool, DbError> {
	let Ok(db) = services.core_db() else {
		return Ok(false);
	};
	let product = product_id.to_string();
	db.read(move |conn| {
		Ok(conn.prepare_cached(SELECT_CONFIRMED)?.exists(named_params! {
			":product": product,
			":resource": <&str>::from(resource),
			":period": period,
		})?)
	})
	.await
}

const UPSERT_CONFIRMED: &str = "INSERT OR REPLACE INTO confirmed_allowance
	(product_id, resource, period) VALUES (:product, :resource, :period)";

/// Records that the wallet confirmed `resource` for `product_id` in
/// `period`, replacing what it recorded before.
pub async fn record(
	services: &RuntimeServices,
	product_id: &ProductId,
	resource: AllowanceResource,
	period: Option<u32>,
) -> Result<(), DbError> {
	let Ok(db) = services.core_db() else {
		return Ok(());
	};
	let product = product_id.to_string();
	db.write(move |tx| {
		tx.prepare_cached(UPSERT_CONFIRMED)?.execute(named_params! {
			":product": product,
			":resource": <&str>::from(resource),
			":period": period,
		})?;
		Ok(())
	})
	.await
}

const DELETE_CONFIRMED: &str = "DELETE FROM confirmed_allowance
	WHERE product_id = :product AND resource = :resource";

/// Removes what the wallet recorded about `resource` for `product_id`.
pub async fn forget(
	services: &RuntimeServices,
	product_id: &ProductId,
	resource: AllowanceResource,
) -> Result<(), DbError> {
	let Ok(db) = services.core_db() else {
		return Ok(());
	};
	let product = product_id.to_string();
	db.write(move |tx| {
		tx.prepare_cached(DELETE_CONFIRMED)?.execute(named_params! {
			":product": product,
			":resource": <&str>::from(resource),
		})?;
		Ok(())
	})
	.await
}
