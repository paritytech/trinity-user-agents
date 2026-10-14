package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoval
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoveError
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.PocketCardRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.RendererNode
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealPocketCollection @Inject constructor(
    private val pinnedPocketCards: PinnedPocketCards,
    private val repository: PocketCardRepository,
) : PocketCardStore {
    /**
     * The settled collection, pinned cards first. Nothing is emitted until the pinned cards are
     * known, because a list without them reads as "this card is not in the Pocket" to the deeplink
     * handler and to the core's own view of the collection.
     */
    override fun observeCards(): Flow<List<PocketCard>> = flow {
        val pinned = pinnedPocketCards.cards().map { it.card }
        val pinnedKeys = pinned.map { it.key }.toSet()

        // A card the user added before the host came to pin it is listed once, as the pinned one.
        emitAll(
            repository.observeCards().map { stored -> pinned + stored.filterNot { it.key in pinnedKeys } }
        )
    }

    override suspend fun removeCard(key: PocketCardKey): Result<PocketRemoval> {
        if (pinnedPocketCards.pinned(key) != null) return Result.failure(PocketRemoveError.Privileged)

        return runCatching { if (repository.delete(key)) PocketRemoval.REMOVED else PocketRemoval.ABSENT }
    }

    override suspend fun addCard(card: CachedPocketCard) {
        require(!card.card.privileged) { "only the host places privileged cards" }
        repository.insert(card)
    }

    /** The newest face the product drew, else the one bundled with a pinned card for its first run. */
    override suspend fun cachedFace(key: PocketCardKey): RendererNode? =
        repository.face(key) ?: pinnedPocketCards.pinned(key)?.face

    // One write against the face alone: a face landing while the card is being removed can no longer
    // put the card back, and the card list does not change every time a product redraws.
    override suspend fun cacheFace(key: PocketCardKey, face: RendererNode) = repository.saveFace(key, face)
}
