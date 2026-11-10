package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import kotlinx.coroutines.flow.Flow
import uniffi.truapi.RendererNode

/** The live side of a face: the product's render stream and the actions going back to it. */
interface PocketFaceStreams {
    /**
     * Faces the product draws for [key], each replacing the previous. Collecting holds one worker
     * reference for the card's product; the flow stays silent while the worker is unavailable.
     */
    fun renderFaces(key: PocketCardKey): Flow<RendererNode>

    fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray)
}
