package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardFaceOnOpen
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds

/** Reads the setting from the product's cached manifest; any lookup that is not quick and clear shows the face. */
class RealPocketCardFaceOnOpen @Inject constructor(
    private val publishedCards: PublishedPocketCards,
) : PocketCardFaceOnOpen {
    override suspend fun faceShownOnOpen(key: PocketCardKey): Boolean = withTimeoutOrNull(LOOKUP_TIMEOUT) {
        publishedCards.find(key.productId, key.cardId).getOrNull()?.definition?.faceShown
    } ?: true

    private companion object {
        // The card is already travelling; past this the page loads regardless.
        val LOOKUP_TIMEOUT = 500.milliseconds
    }
}
