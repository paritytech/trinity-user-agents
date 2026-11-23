package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import kotlinx.coroutines.flow.Flow

/** Where a card's face tree comes from. */
interface PocketFaceSource {
    /**
     * Emits the cached face for [key], if available, followed by live updates.
     * The source closes each face's [PocketFace.materials] when the next face replaces it or collection ends.
     */
    fun observeFace(key: PocketCardKey): Flow<PocketFace>

    /** Delivers a press or edit inside the face to the product. [payload] is empty for a press. */
    fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray)

    /** Where the bytes of an image inside the face come from: the product's archive or the Bulletin gateway. */
    suspend fun resolveImage(key: PocketCardKey, source: JsImageSource): Result<Uri>
}
