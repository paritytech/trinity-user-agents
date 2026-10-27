package io.paritytech.polkadotapp.feature_products_api.domain.game

import uniffi.truapi.ProductId

/** Reminds the player of a product's next game start; each product holds one reminder. */
interface ProductGameReminder {
    /**
     * Remind about [productId]'s game starting at [startsAtMillis] (Unix ms), replacing its own reminder.
     * Asks the OS for what the reminder uses, and fails without holding anything when notifications are not allowed.
     */
    suspend fun schedule(productId: ProductId, startsAtMillis: Long): Result<Unit>

    /** Return the held start time for [productId], if it has a reminder. */
    fun scheduledStartFor(productId: ProductId): Long?

    /** Drop [productId]'s reminder, if any. */
    suspend fun cancel(productId: ProductId)

    /** Drop the reminder at the product's request and report that cancellation. */
    suspend fun cancelByProduct(productId: ProductId)

    /** Re-arm the held reminders after a reboot, dropping those whose start has passed. */
    suspend fun restore()
}
