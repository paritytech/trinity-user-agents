package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.reopenBackoff
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import javax.inject.Inject

class PocketCardWorkers @Inject constructor(
    private val publishedCards: PublishedPocketCards,
    private val runtimes: TrUAPIHostRuntimeProvider,
) {
    suspend fun holdVisibleSides(key: PocketCardKey, expanded: Flow<Boolean>, onReady: () -> Unit) {
        val card = publishedCard(key) ?: return
        val runtime = runtimes.runtime().logFailure("Pocket worker unavailable for ${key.cardId.value}").getOrNull() ?: return

        runtime.hold(key.productId) {
            coroutineScope {
                if (card.definition.back != null) launch {
                    expanded.distinctUntilChanged().collectLatest { isExpanded ->
                        if (isExpanded) runtime.hold(key.productId) { awaitCancellation() }
                    }
                }
                onReady()
                awaitCancellation()
            }
        }
    }

    private suspend fun publishedCard(key: PocketCardKey): PublishedPocketCard? {
        var attempt = 0L
        while (true) {
            val result = publishedCards.find(key.productId, key.cardId)
                .logFailure("Pocket face ${key.cardId.value} has no published card")
            result.getOrNull()?.let { return it }
            val failure = result.exceptionOrNull()
            if (failure is PocketPublishError || failure is ProductResolutionError.MalformedManifest) return null
            delay(reopenBackoff(attempt++))
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
