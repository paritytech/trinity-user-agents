package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_products_impl.domain.product.update.ProductUpdateWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.product.update.ProductUpdater
import javax.inject.Inject

class ProductUpdatesInitializer @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val productUpdater: ProductUpdater,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        productUpdater.start(scope)
        ProductUpdateWorker.schedule(context)
    }
}
