package io.paritytech.polkadotapp.feature_products_impl.domain.bot.e2e

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductMessageRenderObserver
import timber.log.Timber
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

/** Logs the render ack the e2e launcher waits for, once the debug receiver has switched it on. */
@Singleton
class E2ERuntimeMarkers @Inject constructor() : ProductMessageRenderObserver {
    @Volatile
    var enabled: Boolean = false

    override fun onMessageRendered(productId: ProductId) {
        if (enabled) Timber.tag(E2E_LOG_TAG).i(E2EAcks.customRendererUpdate(productId))
    }
}
