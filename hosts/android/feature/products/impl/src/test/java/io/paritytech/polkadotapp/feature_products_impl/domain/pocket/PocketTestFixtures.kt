package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.PocketCardRepository
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import uniffi.truapi.RendererNode

internal val gameProduct = ProductId.fromStoredValue("game.dot")
internal val personhoodProduct = ProductId.fromStoredValue("peopl.dot")

internal fun cardKey(productId: ProductId, cardId: String) = PocketCardKey(productId, PocketCardId(cardId))

internal fun faceOf(text: String): RendererNode = RendererNode.String(text)

internal fun pinnedCard(productId: ProductId, cardId: String) = CachedPocketCard(
    card = PocketCard(cardKey(productId, cardId), title = cardId, privileged = true),
    face = faceOf("pinned $cardId"),
)

internal fun addedCard(productId: ProductId, cardId: String) = CachedPocketCard(
    card = PocketCard(cardKey(productId, cardId), title = cardId, privileged = false),
    face = faceOf("added $cardId"),
)

internal class FakePinnedPocketCards(
    private val cards: List<CachedPocketCard>,
    // The real one awaits the network's name before it can list the cards, and never fails.
    private val listable: Boolean = true,
) : PinnedPocketCards {
    override suspend fun cards(): List<CachedPocketCard> {
        if (!listable) awaitCancellation()

        return cards
    }

    override fun pinned(key: PocketCardKey): CachedPocketCard? = cards.firstOrNull { it.card.key == key }
}

internal class InMemoryPocketCardRepository : PocketCardRepository {
    private val cards = MutableStateFlow<List<PocketCard>>(emptyList())
    private val faces = mutableMapOf<PocketCardKey, RendererNode>()

    override fun observeCards(): Flow<List<PocketCard>> = cards

    override suspend fun insert(card: CachedPocketCard) {
        cards.value = cards.value.filterNot { it.key == card.card.key } + card.card
        saveFace(card.card.key, card.face)
    }

    override suspend fun delete(key: PocketCardKey): Boolean {
        val held = cards.value.any { it.key == key }
        cards.value = cards.value.filterNot { it.key == key }
        faces -= key
        return held
    }

    override suspend fun face(key: PocketCardKey): RendererNode? = faces[key]

    override suspend fun saveFace(key: PocketCardKey, face: RendererNode) {
        faces[key] = face
    }
}
