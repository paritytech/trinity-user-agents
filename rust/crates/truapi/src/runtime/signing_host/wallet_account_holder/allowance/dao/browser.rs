//! A browser host has no core database, so it keeps no confirmed allowance.

use core::convert::Infallible;

use crate::{
	platform::ProductId,
	runtime::{allowances::AllowanceResource, services::RuntimeServices},
};

/// Nothing is confirmed.
pub async fn is_confirmed(
	_services: &RuntimeServices,
	_product_id: &ProductId,
	_resource: AllowanceResource,
	_period: Option<u32>,
) -> Result<bool, Infallible> {
	Ok(false)
}

/// Confirmations are not kept.
pub async fn record(
	_services: &RuntimeServices,
	_product_id: &ProductId,
	_resource: AllowanceResource,
	_period: Option<u32>,
) -> Result<(), Infallible> {
	Ok(())
}

/// Nothing was recorded.
pub async fn forget(
	_services: &RuntimeServices,
	_product_id: &ProductId,
	_resource: AllowanceResource,
) -> Result<(), Infallible> {
	Ok(())
}
