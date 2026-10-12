//! Unified [`Game`] trait.

use crate::versioned::game::{
    HostCancelNextGameError, HostCancelNextGameRequest, HostCancelNextGameResponse,
    HostRemindNextGameError, HostRemindNextGameRequest, HostRemindNextGameResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// Reminders for a product's next game.
#[wire_trait(id = 21)]
#[crate::async_trait]
pub trait Game: Send + Sync {
    /// Remind the user when this product's next game starts.
    ///
    /// Replaces the reminder this product already holds. Served only to the
    /// game product: any other product, or a host that cannot hold reminders,
    /// gets `Unsupported`. A `startsAt` that is not in the future fails with
    /// `StartsInPast`, and a reminder the host cannot hold fails as a host
    /// failure carrying its reason.
    ///
    /// ```ts
    /// const result = await truapi.game.remindNextGame({
    ///   startsAt: BigInt(Date.now() + 60_000),
    /// });
    /// assert(result.isOk(), "remindNextGame failed:", result);
    /// ```
    #[wire(id = 0)]
    async fn remind_next_game(
        &self,
        cx: &CallContext,
        request: HostRemindNextGameRequest,
    ) -> Result<HostRemindNextGameResponse, CallError<HostRemindNextGameError>>;

    /// Drop the reminder. Safe to call whether one is held or not.
    ///
    /// ```ts
    /// const result = await truapi.game.cancelNextGame({});
    /// assert(result.isOk(), "cancelNextGame failed:", result);
    /// ```
    #[wire(id = 1)]
    async fn cancel_next_game(
        &self,
        cx: &CallContext,
        request: HostCancelNextGameRequest,
    ) -> Result<HostCancelNextGameResponse, CallError<HostCancelNextGameError>>;
}
