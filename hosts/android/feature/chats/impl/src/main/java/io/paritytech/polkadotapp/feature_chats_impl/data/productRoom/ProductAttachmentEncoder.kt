package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom

import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentUpload
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.FileTooLargeException
import kotlinx.coroutines.withContext
import javax.inject.Inject

/** An image made smaller to fit a size limit. */
class ShrunkImage(val bytes: ByteArray, val mimeType: String, val width: Int, val height: Int)

/** Encodes an image again, then makes it smaller, until it fits [limitBytes]. Null when it does not fit. */
fun interface ProductImageShrinker {
    fun shrink(image: ByteArray, limitBytes: Int): ShrunkImage?
}

/**
 * Makes inline media of at most 1 MiB (`CHAT_INLINE_MEDIA_MAX_BYTES`, 02-DESIGN §3.5) from an attachment of the user in
 * a product room. An image that is too large is encoded again until it fits. A larger video or file is refused.
 *
 * When the room stages attachments (07-LATER §7), an attachment above 1 MiB and up to 8 MiB becomes a
 * [ProductAttachmentDraft] that keeps the original bytes. When the room seals attachments, the bound is 32 MiB. An image
 * above the bound is encoded again to go inline.
 */
class ProductAttachmentEncoder @Inject constructor(
    private val contentResolver: ContentResolver,
    private val metaBuilder: AttachmentMetaBuilder,
    private val imageShrinker: ProductImageShrinker,
    private val dispatchers: CoroutineDispatchers,
) {
    suspend fun encode(uri: Uri, mimeType: String, size: InformationSize): Result<ChatMediaItemModel> = withContext(dispatchers.io) {
        runCatching {
            when (val meta = metaBuilder.build(uri, mimeType, size)) {
                is Attachment.Meta.Image -> image(uri, meta)

                is Attachment.Meta.Video -> ChatMediaItemModel.Video(
                    resource = ChatResourceModel.Inline(mimeType, readWithinLimit(uri, size).toDataByteArray()),
                    meta = mediaMeta(mimeType, size, width = null, height = null, placeholder = meta.blurHash),
                )

                is Attachment.Meta.General -> {
                    val bytes = readWithinLimit(uri, size)
                    ChatMediaItemModel.File(
                        resource = ChatResourceModel.Inline(mimeType, bytes.toDataByteArray()),
                        meta = mediaMeta(mimeType, bytes.size.bytes, width = null, height = null, placeholder = null),
                        fileName = meta.fileName.ifEmpty { DEFAULT_FILE_NAME },
                    )
                }
            }
        }
    }

    /**
     * True when an attachment of [size] goes through [upload] rather than inline. Every file of a HOP room does, since
     * the native contacts of the chat product read no inline media.
     */
    fun goesToStaging(mimeType: String, size: InformationSize, upload: ProductAttachmentUpload): Boolean =
        upload == ProductAttachmentUpload.HOP ||
            size > LIMIT && (size <= stagedLimit(upload) || !mimeType.startsWith(IMAGE_PREFIX))

    /** Reads an attachment for staging, with its meta. Refuses an attachment above the bound of staging or sealing. */
    suspend fun prepareStaged(
        uri: Uri,
        mimeType: String,
        size: InformationSize,
        upload: ProductAttachmentUpload,
    ): Result<ProductAttachmentDraft> =
        withContext(dispatchers.io) {
            runCatching {
                val limit = stagedLimit(upload)
                if (size > limit) throw FileTooLargeException(size, limit)
                val meta = metaBuilder.build(uri, mimeType, size)
                val bytes = read(uri)
                if (bytes.size > limit.inWholeBytes) throw FileTooLargeException(bytes.size.bytes, limit)

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
                        meta = mediaMeta(mimeType, bytes.size.bytes, width = null, height = null, placeholder = meta.blurHash),
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

    private fun stagedLimit(upload: ProductAttachmentUpload): InformationSize = when (upload) {
        ProductAttachmentUpload.PREIMAGE -> STAGED_LIMIT
        ProductAttachmentUpload.SEALED -> SEALED_LIMIT
        ProductAttachmentUpload.HOP -> HOP_LIMIT
    }

    private fun image(uri: Uri, meta: Attachment.Meta.Image): ChatMediaItemModel.Image {
        val original = read(uri)
        if (original.size <= LIMIT_BYTES) {
            return ChatMediaItemModel.Image(
                resource = ChatResourceModel.Inline(meta.mimeType, original.toDataByteArray()),
                meta = mediaMeta(meta.mimeType, original.size.bytes, meta.width, meta.height, meta.blurHash),
            )
        }

        val shrunk = imageShrinker.shrink(original, LIMIT_BYTES) ?: throw FileTooLargeException(original.size.bytes, LIMIT)
        return ChatMediaItemModel.Image(
            resource = ChatResourceModel.Inline(shrunk.mimeType, shrunk.bytes.toDataByteArray()),
            meta = mediaMeta(shrunk.mimeType, shrunk.bytes.size.bytes, shrunk.width, shrunk.height, meta.blurHash),
        )
    }

    private fun readWithinLimit(uri: Uri, size: InformationSize): ByteArray {
        if (size > LIMIT) throw FileTooLargeException(size, LIMIT)
        return read(uri).also { if (it.size > LIMIT_BYTES) throw FileTooLargeException(it.size.bytes, LIMIT) }
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
        val LIMIT = 1.megabytes
        val LIMIT_BYTES = LIMIT.inWholeBytes.toInt()

        /** The largest attachment that the core stages (`CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES`). */
        val STAGED_LIMIT = 8.megabytes
        val STAGED_LIMIT_BYTES = STAGED_LIMIT.inWholeBytes.toInt()

        /** The largest file that the core uploads to HOP (`CHAT_HOP_FILE_MAX_BYTES`), as the native chat sends. */
        val HOP_LIMIT = 128.megabytes

        /** The largest attachment that the core seals (`CHAT_SEALED_ATTACHMENT_MAX_BYTES`). */
        val SEALED_LIMIT = 32.megabytes

        private const val DEFAULT_FILE_NAME = "file"
        private const val IMAGE_PREFIX = "image/"
    }
}
