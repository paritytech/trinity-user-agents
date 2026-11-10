package io.paritytech.polkadotapp.feature_products_api.domain.pocket

/** Whether a card opens with its face shown, as its product published it. */
interface PocketCardFaceOnOpen {
    /**
     * Answers within a fraction of a second: a card whose product cannot be asked in time opens with
     * the face shown, since a face hidden by mistake is not one the user knows to pull back.
     */
    suspend fun faceShownOnOpen(key: PocketCardKey): Boolean
}
