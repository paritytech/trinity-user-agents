package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_products_api.model.Product
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
    fun create(product: Product): ProductChatExtension {
        return ProductChatExtension(
            appContext = appContext,
            product = product,
            workerRefCounter = workerRefCounter,
        )
    }
}
