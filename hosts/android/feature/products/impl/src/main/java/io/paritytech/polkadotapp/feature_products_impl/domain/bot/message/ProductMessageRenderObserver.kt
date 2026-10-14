package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/** Told each time a product message's widget tree reaches the screen. Bound into a set; main binds none. */
interface ProductMessageRenderObserver {
    fun onMessageRendered(productId: ProductId)
}
