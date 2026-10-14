package io.paritytech.polkadotapp.feature_chats_impl.data.attachment

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import javax.inject.Inject

class GeneralAttachmentMetaBuilder @Inject constructor(
    private val contentAccessor: ContentResolver
) : TypedAttachmentMetaBuilder {
    override val mimeTypePrefix: String = ""

    override suspend fun build(uri: Uri, mimeType: String, size: InformationSize): Attachment.Meta {
        val fileName = contentAccessor.queryFileName(uri).orEmpty()

        return Attachment.Meta.General(
            fileName = fileName,
            mimeType = mimeType,
            size = size
        )
    }
}
