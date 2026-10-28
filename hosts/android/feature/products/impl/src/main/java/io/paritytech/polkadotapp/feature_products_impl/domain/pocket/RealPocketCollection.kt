package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoval
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketRemoveError
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.PocketCardRepository
import kotlinx.coroutines.flow.Flow
import uniffi.truapi.PocketCardRemoval
import uniffi.truapi.RendererNode
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealPocketCollection @Inject constructor(
    private val pinnedPocketCards: PinnedPocketCards,
    private val repository: PocketCardRepository,
) : PocketCardStore {
    override fun observeCards(): Flow<List<PocketCard>> = repository.observeCards()

    override suspend fun removeCard(key: PocketCardKey): Result<PocketRemoval> = runCatching {
        when (repository.remove(key)) {
            PocketCardRemoval.REMOVED -> PocketRemoval.REMOVED
            PocketCardRemoval.ABSENT -> PocketRemoval.ABSENT
            PocketCardRemoval.PRIVILEGED -> throw PocketRemoveError.Privileged
        }
    }

    override suspend fun addCard(card: CachedPocketCard) {
        require(!card.card.privileged) { "only the host places privileged cards" }
        repository.insert(card)
    }

    /** The newest face the product drew, else the one bundled with a pinned card for its first run. */
    override suspend fun cachedFace(key: PocketCardKey): RendererNode? =
        repository.face(key) ?: pinnedPocketCards.pinned(key)?.face
}
