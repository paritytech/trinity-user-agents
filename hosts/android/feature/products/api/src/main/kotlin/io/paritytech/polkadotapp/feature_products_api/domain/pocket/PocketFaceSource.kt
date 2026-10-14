package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import kotlinx.coroutines.flow.Flow

/** Where a card's face tree comes from. */
interface PocketFaceSource {
    /**
     * The face for [key], starting with the cached tree. The backing product's worker is kept
     * running for as long as the flow is collected, so collect it only while the face is on screen.
     */
    fun observeFace(key: PocketCardKey): Flow<JsWidget>

    /** Delivers a press or edit inside the face to the product. [payload] is empty for a press. */
    fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray)

    /** Where the bytes of an image inside the face come from: the product's archive or the Bulletin gateway. */
    suspend fun resolveImage(key: PocketCardKey, source: JsImageSource): Result<Uri>
}
