package io.paritytech.polkadotapp.feature_products_impl.data.pocket

import io.paritytech.polkadotapp.database.dao.PocketCardDao
import io.paritytech.polkadotapp.database.model.PocketCardFaceLocal
import io.paritytech.polkadotapp.database.model.PocketCardLocal
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.CachedPocketCard
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import timber.log.Timber
import uniffi.truapi.RendererNode
import uniffi.truapi.decodeRendererNode
import uniffi.truapi.encodeRendererNode
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Cards the user added, and the newest face held for any card. Host-placed cards have no membership
 * row of their own, but their face is kept here like every other.
 */
interface PocketCardRepository {
    fun observeCards(): Flow<List<PocketCard>>

    suspend fun insert(card: CachedPocketCard)

    /** Whether a card was held under [key]. */
    suspend fun delete(key: PocketCardKey): Boolean

    suspend fun face(key: PocketCardKey): RendererNode?

    suspend fun saveFace(key: PocketCardKey, face: RendererNode)
}

@Singleton
class RealPocketCardRepository @Inject constructor(
    private val dao: PocketCardDao,
) : PocketCardRepository {
    override fun observeCards(): Flow<List<PocketCard>> =
        dao.observeAll().map { cards -> cards.map { it.toDomain() } }

    override suspend fun insert(card: CachedPocketCard) {
        dao.insert(PocketCardLocal(card.card.key.productId.value, card.card.key.cardId.value, card.card.title))
        saveFace(card.card.key, card.face)
    }

    override suspend fun delete(key: PocketCardKey): Boolean {
        dao.deleteFace(key.productId.value, key.cardId.value)

        return dao.delete(key.productId.value, key.cardId.value) > 0
    }

    /**
     * A face the running app can no longer read is answered as none rather than thrown: the card
     * keeps its place and waits for its product to draw again, where a failure here would take down
     * the home tab, the core's card list and the deeplink handler alike, none of which catch.
     */
    override suspend fun face(key: PocketCardKey): RendererNode? {
        val stored = dao.getFace(key.productId.value, key.cardId.value) ?: return null

        return runCatching { decodeRendererNode(stored) }
            .onFailure { Timber.w(it, "pocket: the stored face for %s no longer decodes", key.cardId.value) }
            .getOrNull()
    }

    override suspend fun saveFace(key: PocketCardKey, face: RendererNode) = dao.insertFace(
        PocketCardFaceLocal(
            productId = key.productId.value,
            cardId = key.cardId.value,
            face = encodeRendererNode(face),
        ),
    )

    private fun PocketCardLocal.toDomain() = PocketCard(
        key = PocketCardKey(ProductId.fromStoredValue(productId), PocketCardId(cardId)),
        title = title,
        privileged = false,
    )
}
