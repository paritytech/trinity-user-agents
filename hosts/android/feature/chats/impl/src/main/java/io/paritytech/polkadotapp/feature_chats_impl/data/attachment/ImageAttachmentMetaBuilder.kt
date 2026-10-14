package io.paritytech.polkadotapp.feature_chats_impl.data.attachment

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import timber.log.Timber
import javax.inject.Inject

class ImageAttachmentMetaBuilder @Inject constructor(
    private val contentAccessor: ContentResolver
) : TypedAttachmentMetaBuilder {
    override val mimeTypePrefix: String = "image/"

    override suspend fun build(uri: Uri, mimeType: String, size: InformationSize): Attachment.Meta {
        val imageSize = contentAccessor.getImageSize(uri)

        val blurHash = contentAccessor.loadImagePreview(uri, BLURHASH_PREVIEW_TARGET_PX)
            .flatMap { it.encodeBlurHash() }
            .logFailure("Failed to encode blurhash for image $uri")
            .onSuccess { Timber.d("Calculated blurhash for image $uri (${it.length}) bytes") }
            .getOrNull()

        return Attachment.Meta.Image(
            width = imageSize.width,
            height = imageSize.height,
            blurHash = blurHash,
            mimeType = mimeType,
            size = size
        )
    }
}
