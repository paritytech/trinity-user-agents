package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.awaitCancellation
import javax.inject.Inject

/**
 * Serves a product's chat from its worker on the core instead of the native worker. While [serve]
 * runs, the product's chat is bound for the worker's chat callbacks and one worker reference keeps
 * the worker running; both go when it is cancelled.
 */
class TrUAPIChatHost @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val workers: TrUAPIWorkerSupervisor,
    private val surfaces: TrUAPIChatSurfaces,
) {
    suspend fun serve(
        productId: ProductId,
        extensionId: ChatExtensionId,
        messaging: ProductChatMessaging,
        attachWorker: (ProductWorker?) -> Unit,
    ): Nothing {
        val runtime = runtimeProvider.runtime()
            .logFailure("TrUAPI runtime unavailable; chat of ${productId.value} is not served")
            .getOrElse { awaitCancellation() }

        // Bound before the reference: that reference is what boots the worker, and its first chat
        // call must find the binding.
        surfaces.bind(productId, messaging)
        runtime.acquireWorker(productId.value)
        try {
            attachWorker(TrUAPIChatProductWorker(productId, extensionId, workers))
            awaitCancellation()
        } finally {
            attachWorker(null)
            runtime.releaseWorker(productId.value)
            surfaces.unbind(productId, messaging)
        }
    }
}
