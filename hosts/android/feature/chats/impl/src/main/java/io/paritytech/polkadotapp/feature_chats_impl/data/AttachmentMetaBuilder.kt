package io.paritytech.polkadotapp.feature_chats_impl.data

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.TypedAttachmentMetaBuilder
import kotlinx.coroutines.withContext
import javax.inject.Inject

class AttachmentMetaBuilder @Inject constructor(
    typedBuilders: Set<@JvmSuppressWildcards TypedAttachmentMetaBuilder>,
    private val coroutineDispatchers: CoroutineDispatchers
) {
    // Longest prefix wins so that `image/` / `video/` are matched before the empty-prefix fallback.
    private val buildersByPriority = typedBuilders.sortedByDescending { it.mimeTypePrefix.length }

    suspend fun build(uri: Uri, mimeType: String, size: InformationSize): Attachment.Meta =
        withContext(coroutineDispatchers.io) {
            val builder = buildersByPriority.first { mimeType.startsWith(it.mimeTypePrefix) }
            builder.build(uri, mimeType, size)
        }
}
