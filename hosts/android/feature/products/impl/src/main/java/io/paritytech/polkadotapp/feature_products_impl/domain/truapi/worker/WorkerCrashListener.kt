package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import uniffi.truapi.ProductId

/** Tells the user of a product's crashing worker in its own screens; answers `false` to leave it to the generic notice. */
fun interface WorkerCrashListener {
    fun onWorkerCrashing(productId: ProductId): Boolean
}
