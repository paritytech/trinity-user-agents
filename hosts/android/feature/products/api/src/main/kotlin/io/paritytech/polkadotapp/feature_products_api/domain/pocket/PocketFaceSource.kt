package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import kotlinx.coroutines.flow.Flow

/** Where a card's face tree comes from. */
interface PocketFaceSource {
    /**
     * Bind a visible card, starting with the cached face. The card holds its worker until the
     * binding ends. An expanded card also holds a reference for its published back.
     * The source owns each face's [PocketFace.materials]: it closes one when the next face replaces it
     * and the last when collection ends.
     */
    fun bindVisibleCard(key: PocketCardKey, expanded: Flow<Boolean>): Flow<PocketFace>

    /** Delivers a press or edit inside the face to the product. [payload] is empty for a press. */
    fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray)

    /** Where the bytes of an image inside the face come from: the product's archive or the Bulletin gateway. */
    suspend fun resolveImage(key: PocketCardKey, source: JsImageSource): Result<Uri>
}
