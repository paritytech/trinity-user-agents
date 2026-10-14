package io.paritytech.polkadotapp.feature_chats_impl.data.attachment

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment

/**
 * Builds [Attachment.Meta] for a specific media family, selected by mime-type prefix
 * (e.g. `"image/"`, `"video/"`). The empty prefix acts as a catch-all fallback.
 */
interface TypedAttachmentMetaBuilder {
    val mimeTypePrefix: String

    suspend fun build(uri: Uri, mimeType: String, size: InformationSize): Attachment.Meta
}
