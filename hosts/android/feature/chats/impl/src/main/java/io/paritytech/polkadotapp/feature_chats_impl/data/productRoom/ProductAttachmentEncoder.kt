package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.FileTooLargeException
import kotlinx.coroutines.withContext
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/** Reads an attachment of the user in a room of the chat product, with its meta, for the upload to HOP. */
class ProductAttachmentEncoder @Inject constructor(
    private val contentResolver: ContentResolver,
    private val metaBuilder: AttachmentMetaBuilder,
    private val dispatchers: CoroutineDispatchers,
) {
    /** Refuses an attachment above [HOP_LIMIT], as the native chat does. */
    suspend fun prepare(uri: Uri, mimeType: String, size: InformationSize): Result<ProductAttachmentDraft> =
        withContext(dispatchers.io) {
            runCatching {
                if (size > HOP_LIMIT) throw FileTooLargeException(size, HOP_LIMIT)
                val meta = metaBuilder.build(uri, mimeType, size)
                val bytes = read(uri)
                if (bytes.size > HOP_LIMIT.inWholeBytes) throw FileTooLargeException(bytes.size.bytes, HOP_LIMIT)

                when (meta) {
                    is Attachment.Meta.Image -> ProductAttachmentDraft(
                        kind = ProductAttachmentDraft.Kind.IMAGE,
                        bytes = bytes,
                        meta = mediaMeta(mimeType, bytes.size.bytes, meta.width, meta.height, meta.blurHash),
                        fileName = null,
                    )

                    is Attachment.Meta.Video -> ProductAttachmentDraft(
                        kind = ProductAttachmentDraft.Kind.VIDEO,
                        bytes = bytes,
                        meta = mediaMeta(mimeType, bytes.size.bytes, width = null, height = null, placeholder = meta.blurHash)
                            .copy(durationMs = meta.duration.seconds.inWholeMilliseconds),
                        fileName = null,
                    )

                    is Attachment.Meta.General -> ProductAttachmentDraft(
                        kind = ProductAttachmentDraft.Kind.FILE,
                        bytes = bytes,
                        meta = mediaMeta(mimeType, bytes.size.bytes, width = null, height = null, placeholder = null),
                        fileName = meta.fileName.ifEmpty { DEFAULT_FILE_NAME },
                    )
                }
            }
        }

    private fun read(uri: Uri): ByteArray =
        checkNotNull(contentResolver.openInputStream(uri)) { "The attachment cannot be read" }.use { it.readBytes() }

    private fun mediaMeta(mimeType: String, size: InformationSize, width: Int?, height: Int?, placeholder: String?) = ChatMediaMetaModel(
        mimeType = mimeType,
        sizeBytes = size.inWholeBytes,
        width = width,
        height = height,
        placeholder = placeholder,
        alt = null,
    )

    companion object {
        /** The largest file that the core uploads to HOP (`CHAT_HOP_FILE_MAX_BYTES`), as the native chat sends. */
        val HOP_LIMIT = 128.megabytes

        private const val DEFAULT_FILE_NAME = "file"
    }
}
