package io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e

const val E2E_LOG_TAG = "truapi.e2e"

object E2EAcks {
    const val SEED_IDENTITY_DONE = "seed_identity done"

    fun productRegistered(productId: String) = "product registered id=$productId"

    fun messageSent(productId: String, roomId: String) = "message sent product=$productId room=$roomId"

    fun customRendererUpdate(productId: String) = "custom_renderer_update product=$productId"

    fun error(hook: String, reason: String) = "error $hook $reason"
}
