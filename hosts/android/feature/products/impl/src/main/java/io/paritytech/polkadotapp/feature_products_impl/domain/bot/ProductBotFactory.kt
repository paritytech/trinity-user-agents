package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.external.HostPlacedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Factory for creating [ProductChatExtension] instances. The worker each extension drives is owned
 * by [ProductWorkerRefCounter], not built here.
 */
@Singleton
class ProductBotFactory @Inject constructor(
    @param:ApplicationContext private val appContext: Context,
    private val workerRefCounter: ProductWorkerRefCounter,
) {
    /** [placedRoom] is the room the host draws before the product has run; see [HostPlacedProduct]. */
    fun create(product: Product, placedRoom: ProductChatIdParameter? = null): ProductChatExtension {
        return ProductChatExtension(
            appContext = appContext,
            product = product,
            workerRefCounter = workerRefCounter,
            placedRoom = placedRoom,
        )
    }
}
