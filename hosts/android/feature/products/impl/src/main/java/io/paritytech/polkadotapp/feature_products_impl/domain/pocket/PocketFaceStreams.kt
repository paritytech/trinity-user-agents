package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import kotlinx.coroutines.flow.Flow
import uniffi.truapi.RendererNode

/** The live side of a face: the product's render stream and the actions going back to it. */
interface PocketFaceStreams {
    /**
     * Faces the product draws for [key], each replacing the previous. Collecting holds one worker
     * reference for the card's product; the flow stays silent while the worker is unavailable.
     * What it learns about the worker goes to [load], for the face's load telemetry only.
     */
    fun renderFaces(key: PocketCardKey, load: PocketFaceLoad): Flow<RendererNode>

    fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray)
}

/** What a face stream learns about the worker that draws the face. */
interface PocketFaceLoad {
    /** The face is asked of the worker; [warm] when it was already running. */
    fun worker(warm: Boolean, sessionId: () -> String?)

    /** No worker will draw this face: the product draws no card, or its worker failed to start. */
    fun noWorker()
}
