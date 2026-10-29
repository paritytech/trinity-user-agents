package io.paritytech.polkadotapp.feature_products_impl.data.pocket

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.CachedPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.reopenAfterFailure
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.PocketEntry
import uniffi.truapi.RendererNode
import javax.inject.Inject
import javax.inject.Singleton

/** The collection the core keeps: the cards in the Pocket, pinned first, and the newest face of each. */
interface PocketCardRepository {
    fun observeCards(): Flow<List<PocketCard>>

    suspend fun insert(card: CachedPocketCard)

    /** Removes a card unless the host pins it. */
    suspend fun remove(key: PocketCardKey)

    suspend fun face(key: PocketCardKey): RendererNode?
}

@Singleton
class CorePocketCardRepository @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : PocketCardRepository {
    // The runtime boots on first use and can fail to; the list waits and asks again rather than
    // leaving the Pocket empty for the rest of the session.
    override fun observeCards(): Flow<List<PocketCard>> =
        flow { emitAll(runtime().pocketCards()) }
            .map { entries -> entries.map { it.toDomain() } }
            .reopenAfterFailure("Pocket collection")

    override suspend fun insert(card: CachedPocketCard) {
        val key = card.card.key
        runtime().addPocketCard(key.productId.value, key.cardId.value, card.card.title, card.face)
    }

    override suspend fun remove(key: PocketCardKey) {
        runtime().removePocketCard(key.productId.value, key.cardId.value)
    }

    override suspend fun face(key: PocketCardKey): RendererNode? =
        runtime().pocketCardFace(key.productId.value, key.cardId.value)

    private suspend fun runtime(): TrUAPIHostRuntime = runtimeProvider.runtime().getOrThrow()

    private fun PocketEntry.toDomain() = PocketCard(
        key = PocketCardKey(ProductId.fromStoredValue(productId), PocketCardId(cardId)),
        title = title,
        privileged = privileged,
    )
}
