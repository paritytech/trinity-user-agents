//! Product-facing Game capability adapter.

use std::sync::Arc;

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::Game,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		game::{
			HostCancelNextGameError, HostCancelNextGameRequest, HostCancelNextGameResponse,
			HostRemindNextGameError, HostRemindNextGameRequest, HostRemindNextGameResponse,
		},
	},
};

use super::from_latest_error;
use crate::{
	platform::GamePlatform,
	runtime::{ProductConnection, ProductRuntimeHost},
	unix_time::current_unix_secs,
};

/// A reminder for a game that has begun brings nobody back. Whole seconds are
/// precise enough for that.
fn ensure_upcoming(starts_at: u64) -> Result<(), CallError<latest::HostRemindNextGameError>> {
	if starts_at > current_unix_secs() * 1000 {
		Ok(())
	} else {
		Err(CallError::Domain(latest::HostRemindNextGameError::StartsInPast))
	}
}

impl ProductConnection {
	/// The host's Game adapter, for the game product only. Any other product,
	/// or a host without an adapter, gets `Unsupported` before anything else
	/// runs. Serving one product is why the API asks for no per-product
	/// consent: the host asks the OS for whatever the reminder needs.
	fn game_platform<E>(&self) -> Result<Arc<dyn GamePlatform>, CallError<E>> {
		if !self.product.is_game_product() {
			return Err(CallError::Unsupported);
		}
		self.game_platform.clone().ok_or(CallError::Unsupported)
	}
}

#[truapi::async_trait]
impl Game for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "game.remind_next_game"))]
	async fn remind_next_game(
		&self,
		cx: &CallContext,
		request: HostRemindNextGameRequest,
	) -> Result<HostRemindNextGameResponse, CallError<HostRemindNextGameError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform = self.connection.game_platform()?;
			let latest::HostRemindNextGameRequest { starts_at } = request;
			ensure_upcoming(starts_at)?;
			if cx.cancel().is_cancelled() {
				return Err(CallError::Cancelled);
			}
			platform
				.schedule_game_reminder(&self.connection.product, starts_at)
				.await
				.map_err(|error| CallError::HostFailure { reason: error.reason })
		}
		.await;
		result
			.map(|response| HostRemindNextGameResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "game.cancel_next_game"))]
	async fn cancel_next_game(
		&self,
		_cx: &CallContext,
		request: HostCancelNextGameRequest,
	) -> Result<HostCancelNextGameResponse, CallError<HostCancelNextGameError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform = self.connection.game_platform()?;
			let latest::HostCancelNextGameRequest {} = request;
			platform
				.cancel_game_reminder(&self.connection.product)
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| HostCancelNextGameResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}
