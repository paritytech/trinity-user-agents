package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardWorkers
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.truapi.ProductId
import javax.inject.Inject

class RealPocketCardWorkers @Inject constructor(
    private val publishedCards: PublishedPocketCards,
    private val runtimes: TrUAPIHostRuntimeProvider,
) : PocketCardWorkers {
    override suspend fun holdVisibleSides(key: PocketCardKey, expanded: Flow<Boolean>) {
        val card = publishedCards.awaitPublished(key) ?: return
        val runtime = runtimes.runtime().logFailure("Pocket worker unavailable for ${key.cardId.value}").getOrNull() ?: return

        runtime.hold(key.productId) {
            if (card.definition.back != null) {
                expanded.distinctUntilChanged().collectLatest { isExpanded ->
                    if (isExpanded) runtime.hold(key.productId) { awaitCancellation() }
                }
            }
            awaitCancellation()
        }
    }

    private suspend fun TrUAPIHostRuntime.hold(productId: ProductId, work: suspend () -> Unit) {
        val claim = claimWorker(productId)
        try {
            work()
        } finally {
            claim.release()
        }
    }
}
