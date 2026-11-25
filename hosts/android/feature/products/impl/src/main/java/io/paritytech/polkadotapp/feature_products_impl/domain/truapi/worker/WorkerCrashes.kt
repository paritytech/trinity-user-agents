package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class WorkerCrashes @Inject constructor() {
    private val crashing = MutableSharedFlow<ProductId>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    // Products whose worker keeps crashing; the core goes on restarting it with backoff. Never ends.
    val products: Flow<ProductId> = crashing

    // May be called on the core's thread.
    fun report(productId: ProductId) {
        crashing.tryEmit(productId)
    }

    private companion object {
        const val BUFFER = 16
    }
}
