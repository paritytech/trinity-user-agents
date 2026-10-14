package io.paritytech.polkadotapp.feature_chats_impl.data.attachment

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.orZero
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import javax.inject.Inject

class VideoAttachmentMetaBuilder @Inject constructor(
    private val contentAccessor: ContentResolver
) : TypedAttachmentMetaBuilder {
    override val mimeTypePrefix: String = "video/"

    override suspend fun build(uri: Uri, mimeType: String, size: InformationSize): Attachment.Meta {
        val duration = contentAccessor.getVideoDuration(uri).orZero()

        val blurHash = contentAccessor.loadVideoPreview(uri, BLURHASH_PREVIEW_TARGET_PX)
            .flatMap { it.encodeBlurHash() }
            .logFailure("Failed to encode blurhash for video $uri")
            .getOrNull()

        return Attachment.Meta.Video(
            duration = duration.inWholeSeconds,
            blurHash = blurHash,
            mimeType = mimeType,
            size = size
        )
    }
}
