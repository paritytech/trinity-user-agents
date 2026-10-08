//! Logging Game host for the CLI.
//!
//! Reminders are accepted and logged but never held or fired: this exists to
//! make a Game product runnable headlessly, not to ring anything.

use truapi::latest::GenericError;
use truapi::platform::{GamePlatform, ProductContext, async_trait};

/// A Game host that accepts every reminder and cancel.
pub struct CliGameHost;

#[async_trait]
impl GamePlatform for CliGameHost {
    async fn schedule_game_reminder(
        &self,
        product: &ProductContext,
        starts_at: u64,
    ) -> Result<(), GenericError> {
        tracing::info!(
            product = %product.product_id,
            starts_at,
            "game reminder accepted"
        );
        Ok(())
    }

    async fn cancel_game_reminder(&self, product: &ProductContext) -> Result<(), GenericError> {
        tracing::info!(product = %product.product_id, "game reminder cancelled");
        Ok(())
    }
}
