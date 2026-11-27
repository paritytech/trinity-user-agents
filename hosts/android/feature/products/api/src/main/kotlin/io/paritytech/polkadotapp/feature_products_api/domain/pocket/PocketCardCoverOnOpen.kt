package io.paritytech.polkadotapp.feature_products_api.domain.pocket

/** Whether a card opens covered by its product's page, as its product published it. */
interface PocketCardCoverOnOpen {
    /**
     * Answers within a fraction of a second: a card whose product cannot be asked in time opens
     * uncovered, since a card covered by mistake is not one the user knows to pull back.
     */
    suspend fun startsCovered(key: PocketCardKey): Boolean
}
