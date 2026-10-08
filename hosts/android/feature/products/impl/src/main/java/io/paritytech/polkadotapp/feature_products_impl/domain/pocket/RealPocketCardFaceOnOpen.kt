package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardFaceOnOpen
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/** Reads the setting from the product's manifest; any lookup that does not answer clearly shows the face. */
class RealPocketCardFaceOnOpen @Inject constructor(
    private val publishedCards: PublishedPocketCards,
) : PocketCardFaceOnOpen {
    override suspend fun faceShownOnOpen(key: PocketCardKey): Boolean = withTimeoutOrNull(LOOKUP_TIMEOUT) {
        publishedCards.find(key.productId, key.cardId).getOrNull()?.definition?.faceShown
    } ?: true

    private companion object {
        // The page waits on the same product resolution before it can be served, so this only bounds
        // a chain read that hangs.
        val LOOKUP_TIMEOUT = 5.seconds
    }
}
