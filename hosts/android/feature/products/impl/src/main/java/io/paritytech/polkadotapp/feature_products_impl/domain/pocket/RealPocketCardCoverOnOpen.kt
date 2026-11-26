package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardCoverOnOpen
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/** Reads the setting from the product's manifest; any lookup that does not answer clearly leaves the card uncovered. */
class RealPocketCardCoverOnOpen @Inject constructor(
    private val publishedCards: PublishedPocketCards,
) : PocketCardCoverOnOpen {
    override suspend fun startsCovered(key: PocketCardKey): Boolean = withTimeoutOrNull(LOOKUP_TIMEOUT) {
        publishedCards.find(key.productId, key.cardId).getOrNull()?.definition?.startsCovered
    } ?: false

    private companion object {
        // The page waits on the same product resolution before it can be served, so this only bounds
        // a chain read that hangs.
        val LOOKUP_TIMEOUT = 5.seconds
    }
}
