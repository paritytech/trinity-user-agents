//! Product-facing Game capability adapter.

use std::sync::Arc;

use tracing::instrument;
use truapi::api::Game;
use truapi::latest;
use truapi::versioned::IntoLatest;
use truapi::versioned::game::{
    HostCancelNextGameError, HostCancelNextGameRequest, HostCancelNextGameResponse,
    HostRemindNextGameError, HostRemindNextGameRequest, HostRemindNextGameResponse,
};
use truapi::{CallContext, CallError};

use crate::platform::GamePlatform;
use crate::runtime::ProductRuntimeHost;
use crate::unix_time::current_unix_secs;

/// A reminder for a game that has begun brings nobody back. Whole seconds are
/// precise enough for that.
fn ensure_upcoming(starts_at: u64) -> Result<(), CallError<HostRemindNextGameError>> {
    if starts_at > current_unix_secs() * 1000 {
        Ok(())
    } else {
        Err(CallError::Domain(HostRemindNextGameError::V1(
            latest::HostRemindNextGameError::StartsInPast,
        )))
    }
}

impl ProductRuntimeHost {
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
        let platform = self.game_platform()?;
        let latest::HostRemindNextGameRequest { starts_at } = request.into_latest();
        ensure_upcoming(starts_at)?;
        if cx.cancel().is_cancelled() {
            return Err(CallError::Cancelled);
        }
        platform
            .schedule_game_reminder(&self.product, starts_at)
            .await
            .map(|()| HostRemindNextGameResponse::V1)
            .map_err(|error| CallError::HostFailure {
                reason: error.reason,
            })
    }

    #[instrument(skip_all, fields(runtime.method = "game.cancel_next_game"))]
    async fn cancel_next_game(
        &self,
        _cx: &CallContext,
        request: HostCancelNextGameRequest,
    ) -> Result<HostCancelNextGameResponse, CallError<HostCancelNextGameError>> {
        let platform = self.game_platform()?;
        let latest::HostCancelNextGameRequest {} = request.into_latest();
        platform
            .cancel_game_reminder(&self.product)
            .await
            .map(|()| HostCancelNextGameResponse::V1)
            .map_err(|error| CallError::Domain(HostCancelNextGameError::V1(error)))
    }
}
