package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.PocketHostBridge
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoval
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoveError
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import uniffi.truapi.NativePocketRemoval
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import uniffi.truapi.PocketCard as NativePocketCard

/**
 * Serves one product's slice of the collection to the core. The list is answered inline from a
 * snapshot; a removal suspends until the store has removed the card, because the core reads the list
 * again right after it and republishes that answer.
 */
class ProductPocketHostBridge(
    private val productId: ProductId,
    private val store: PocketCardStore,
    private val scope: CoroutineScope,
) : PocketHostBridge {
    private val snapshot = AtomicReference<List<NativePocketCard>>(emptyList())
    private var collector: Job? = null
    private val stopped = AtomicBoolean(false)

    /**
     * Keeps the snapshot current and republishes the product's cards on every collection change.
     * Changes that leave this product's slice as it was are not republished: a face streaming at
     * frame rate changes the stored collection continuously without changing any card the core knows.
     */
    fun start(republish: (List<NativePocketCard>) -> Unit) {
        collector = scope.launch {
            store.observeCards()
                .map { cards -> cards.filter { it.key.productId == productId }.map { it.toNative() } }
                .distinctUntilChanged()
                .collect { cards ->
                    snapshot.set(cards)
                    // Cancelling does not wait, and the caller closes the execution as soon as it
                    // returns: a republish already in flight would otherwise reach a freed handle.
                    if (!stopped.get()) republish(cards)
                }
        }
    }

    /** Ends the republishing; [republish] is handed the execution, which its owner is about to close. */
    fun stop() {
        stopped.set(true)
        collector?.cancel()
        collector = null
    }

    override fun listCards(): List<NativePocketCard> = snapshot.get()

    override suspend fun removeCard(cardId: String): NativePocketRemoval {
        // The snapshot carries the flag, so a card the host placed is refused without reaching the store.
        if (snapshot.get().any { it.cardId == cardId && it.privileged }) return NativePocketRemoval.PRIVILEGED

        val removal = store.removeCard(PocketCardKey(productId, PocketCardId(cardId)))
        return removal.fold(
            onSuccess = { outcome ->
                snapshot.updateAndGet { cards -> cards.filterNot { it.cardId == cardId } }
                outcome.toNative()
            },
            onFailure = { failure ->
                if (failure is PocketRemoveError.Privileged) NativePocketRemoval.PRIVILEGED else throw failure
            },
        )
    }

    private fun PocketRemoval.toNative() = when (this) {
        PocketRemoval.REMOVED -> NativePocketRemoval.REMOVED
        PocketRemoval.ABSENT -> NativePocketRemoval.ABSENT
    }

    private fun PocketCard.toNative() = NativePocketCard(cardId = key.cardId.value, privileged = privileged)
}
