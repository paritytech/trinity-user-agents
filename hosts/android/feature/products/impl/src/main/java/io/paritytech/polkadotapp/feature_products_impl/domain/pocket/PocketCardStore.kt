package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCollection
import uniffi.truapi.RendererNode

/** The collection as the host's own flows see it: the public read side plus the writes only the host makes. */
interface PocketCardStore : PocketCollection {
    /** Inserts a card the user approved, replacing the face of one already present. */
    suspend fun addCard(card: CachedPocketCard)

    /** The newest face held for [key]: bundled for a pinned card, approved for an added one, or the last streamed. */
    suspend fun cachedFace(key: PocketCardKey): RendererNode?

    /** Remembers the newest face the product streamed, so the card has it offline and at cold start. */
    suspend fun cacheFace(key: PocketCardKey, face: RendererNode)
}
