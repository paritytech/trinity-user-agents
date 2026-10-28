package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.PocketCardRepository
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

internal class FakePinnedPocketCards(private val cards: List<CachedPocketCard>) : PinnedPocketCards {
    override fun cards(tld: DotNsTld): List<PocketCard> = cards.map { it.card }

    override fun pinned(key: PocketCardKey): CachedPocketCard? = cards.firstOrNull { it.card.key == key }

    override fun bundlesImage(productId: ProductId, path: String): Boolean = false
}

/** The collection as the core keeps it: [pinned] first and removable by nobody, then added cards. */
internal class InMemoryPocketCardRepository(pinned: List<PocketCard> = emptyList()) : PocketCardRepository {
    private val cards = MutableStateFlow(pinned)
    private val faces = mutableMapOf<PocketCardKey, RendererNode>()

    override fun observeCards(): Flow<List<PocketCard>> = cards

    override suspend fun insert(card: CachedPocketCard) {
        if (cards.value.none { it.key == card.card.key }) cards.value += card.card
        faces[card.card.key] = card.face
    }

    override suspend fun remove(key: PocketCardKey) {
        val held = cards.value.firstOrNull { it.key == key } ?: return
        if (held.privileged) return
        cards.value -= held
        faces -= key
    }

    override suspend fun face(key: PocketCardKey): RendererNode? = faces[key]
}
