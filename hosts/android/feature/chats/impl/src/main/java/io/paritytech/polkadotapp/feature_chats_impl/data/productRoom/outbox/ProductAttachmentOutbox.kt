package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatResourceLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toLocal
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber
import java.io.File
import java.util.Properties
import java.util.UUID

/** An attachment above the inline bound that the host stages as a preimage, with the meta of its media item. */
class ProductAttachmentDraft(
    val kind: Kind,
    val bytes: ByteArray,
    val meta: ChatMediaMetaModel,
    /** The name of a file. Null for an image and a video. */
    val fileName: String?,
) {
    enum class Kind { IMAGE, VIDEO, FILE }
}

/** Why an attachment of the outbox was not sent. */
sealed interface ProductAttachmentFailure {
    /** The core or the chain does not store an attachment of this size. */
    data class TooLarge(val maxBytes: Long) : ProductAttachmentFailure

    data object NotDeclared : ProductAttachmentFailure

    data object PermissionDenied : ProductAttachmentFailure

    data class AllowanceUnavailable(val reason: String) : ProductAttachmentFailure

    /** The submission or the delivery to the product failed. A retry can succeed. */
    data class Failed(val reason: String) : ProductAttachmentFailure

    /** The product refused the message. [reason] is for the user. */
    data class Refused(val reason: String?) : ProductAttachmentFailure
}

/** Gives the kept file of an outbox entry for its preview address, so the room draws the attachment before it is sent. */
fun interface ChatOutboxPreviews {
    /** Null for an address that names no kept file of the outbox. */
    fun file(url: String): File?
}

/** An attachment that waits in the outbox until the host sends it, or until the user deletes it. */
data class ProductOutboxEntry(
    /** Starts with [ProductAttachmentOutbox.ID_PREFIX]. The room screen shows the entry with this message id. */
    val id: ChatMessageId,
    val chatId: ChatId,
    /** Milliseconds since the Unix epoch. */
    val createdAt: Long,
    val kind: ProductAttachmentDraft.Kind,
    val meta: ChatMediaMetaModel,
    val fileName: String?,
    /** The caption that the user typed. */
    val text: String?,
    val replyTo: ChatMessageId?,
    /** The key of the preimage, after the staging. A retry then only sends the message. */
    val key: DataByteArray?,
    /** Null while the host sends the attachment. */
    val failure: ProductAttachmentFailure?,
    /** Where the host puts the file before the product gets it. */
    val upload: ProductAttachmentUpload,
    /** The sealed file, after the sealing. A retry then only sends the message. */
    val sealed: ChatResourceModel.Sealed?,
    /** The HOP file, after the upload. A retry then only sends the message. */
    val hopFile: ChatResourceModel.Hop?,
) {
    val isFailed: Boolean
        get() = failure != null
}

/** Where the host puts a user attachment before the product gets it, by what the room declares. */
enum class ProductAttachmentUpload {
    /** One plain preimage that the product pays for (`preimage_attachments`). */
    PREIMAGE,

    /** Encrypted Bulletin parts that the product pays for (`sealed_attachments`). */
    SEALED,

    /** A HOP node of the person chat, as the native chat sends a file (`hop_attachments`). */
    HOP,
}

/**
 * Keeps the attachments that the host stages until it sends them (07-LATER §7, D27). Each entry is a folder with the
 * bytes and a properties file, so an entry stays after the room screen closes and after the process stops.
 */
class ProductAttachmentOutbox(
    private val root: File,
    private val io: CoroutineDispatcher,
    private val now: () -> Long,
) : ChatOutboxPreviews {
    private val mutex = Mutex()
    private val entries = MutableStateFlow<Map<ChatMessageId, ProductOutboxEntry>?>(null)

    /** The entries of a room, the oldest first. */
    fun observe(chatId: ChatId): Flow<List<ProductOutboxEntry>> = entries
        .onStart { loaded() }
        .filterNotNull()
        .map { all -> all.values.filter { it.chatId == chatId }.sortedBy { it.createdAt } }
        .distinctUntilChanged()

    suspend fun all(): List<ProductOutboxEntry> = loaded().values.sortedBy { it.createdAt }

    suspend fun get(id: ChatMessageId): ProductOutboxEntry? = loaded()[id]

    suspend fun add(
        chatId: ChatId,
        draft: ProductAttachmentDraft,
        text: String?,
        replyTo: ChatMessageId?,
        upload: ProductAttachmentUpload,
    ): ProductOutboxEntry =
        mutex.withLock {
            val entry = ProductOutboxEntry(
                id = ID_PREFIX + UUID.randomUUID(),
                chatId = chatId,
                createdAt = now(),
                kind = draft.kind,
                meta = draft.meta,
                fileName = draft.fileName,
                text = text,
                replyTo = replyTo,
                key = null,
                failure = null,
                upload = upload,
                sealed = null,
                hopFile = null,
            )
            withContext(io) {
                val folder = folderOf(entry.id).apply { mkdirs() }
                File(folder, BYTES_FILE).writeBytes(draft.bytes)
                write(entry)
            }
            publish(loadedLocked() + (entry.id to entry))
            entry
        }

    /** Changes an entry and keeps the change. Null when the entry is gone. */
    suspend fun update(id: ChatMessageId, transform: (ProductOutboxEntry) -> ProductOutboxEntry): ProductOutboxEntry? =
        mutex.withLock {
            val current = loadedLocked()
            val entry = current[id] ?: return@withLock null
            val updated = transform(entry)
            withContext(io) { write(updated) }
            publish(current + (id to updated))
            updated
        }

    /** Removes an entry and its bytes. */
    suspend fun remove(id: ChatMessageId) = mutex.withLock {
        val current = loadedLocked()
        withContext(io) { folderOf(id).deleteRecursively() }
        publish(current - id)
    }

    suspend fun bytes(id: ChatMessageId): ByteArray? = withContext(io) {
        File(folderOf(id), BYTES_FILE).takeIf { it.isFile }?.readBytes()
    }

    /** The file that holds the bytes of an entry. */
    fun bytesFile(id: ChatMessageId): File = File(folderOf(id), BYTES_FILE)

    /** The address that the bubble of an entry draws until the host sends the attachment. */
    fun previewUrl(id: ChatMessageId): String = FILE_SCHEME + bytesFile(id).absolutePath

    // Only a kept file of the outbox: an address that a product gives cannot make the host read another file.
    override fun file(url: String): File? {
        if (!url.startsWith(FILE_SCHEME)) return null
        val file = File(url.removePrefix(FILE_SCHEME)).canonicalFile
        val inOutbox = file.parentFile?.parentFile == root.canonicalFile && file.name == BYTES_FILE
        return file.takeIf { inOutbox && it.isFile }
    }

    private suspend fun loaded(): Map<ChatMessageId, ProductOutboxEntry> = entries.value ?: mutex.withLock { loadedLocked() }

    private suspend fun loadedLocked(): Map<ChatMessageId, ProductOutboxEntry> {
        entries.value?.let { return it }
        val read = withContext(io) { readAll() }
        entries.value = read
        return read
    }

    private fun publish(value: Map<ChatMessageId, ProductOutboxEntry>) {
        entries.value = value
    }

    private fun readAll(): Map<ChatMessageId, ProductOutboxEntry> {
        val folders = root.listFiles { file -> file.isDirectory }.orEmpty()
        return folders.mapNotNull { folder ->
            runCatching { read(folder) }
                .onFailure { Timber.w(it, "An entry of the attachment outbox cannot be read; it is removed") }
                .getOrNull()
                ?: run {
                    folder.deleteRecursively()
                    null
                }
        }.associateBy { it.id }
    }

    private fun folderOf(id: ChatMessageId): File = File(root, id.removePrefix(ID_PREFIX))

    private fun write(entry: ProductOutboxEntry) {
        val properties = Properties()
        properties[ID] = entry.id
        properties[CHAT_ID] = entry.chatId.value.value.toHexString()
        properties[CREATED_AT] = entry.createdAt.toString()
        properties[KIND] = entry.kind.name
        properties[MIME_TYPE] = entry.meta.mimeType
        entry.meta.sizeBytes?.let { properties[SIZE_BYTES] = it.toString() }
        entry.meta.width?.let { properties[WIDTH] = it.toString() }
        entry.meta.height?.let { properties[HEIGHT] = it.toString() }
        entry.meta.placeholder?.let { properties[PLACEHOLDER] = it }
        entry.fileName?.let { properties[FILE_NAME] = it }
        entry.text?.let { properties[TEXT] = it }
        entry.replyTo?.let { properties[REPLY_TO] = it }
        entry.key?.let { properties[KEY] = it.value.toHexString() }
        properties[UPLOAD] = entry.upload.name
        entry.sealed?.let { properties[SEALED] = BinaryScale.encodeToByteArray(it.toLocal()).toHexString() }
        entry.hopFile?.let { properties[HOP_FILE] = BinaryScale.encodeToByteArray(it.toLocal()).toHexString() }
        entry.failure?.let { failure -> writeFailure(properties, failure) }

        val folder = folderOf(entry.id).apply { mkdirs() }
        val temporary = File(folder, "$ENTRY_FILE.tmp")
        temporary.outputStream().use { properties.store(it, null) }
        check(temporary.renameTo(File(folder, ENTRY_FILE))) { "The outbox entry ${entry.id} cannot be written" }
    }

    private fun read(folder: File): ProductOutboxEntry? {
        val file = File(folder, ENTRY_FILE)
        if (!file.isFile || !File(folder, BYTES_FILE).isFile) return null
        val properties = Properties().apply { file.inputStream().use(::load) }
        fun value(name: String): String? = properties.getProperty(name)

        return ProductOutboxEntry(
            id = requireNotNull(value(ID)),
            chatId = ChatId.fromRawValue(requireNotNull(value(CHAT_ID)).fromHex()),
            createdAt = requireNotNull(value(CREATED_AT)).toLong(),
            kind = ProductAttachmentDraft.Kind.valueOf(requireNotNull(value(KIND))),
            meta = ChatMediaMetaModel(
                mimeType = requireNotNull(value(MIME_TYPE)),
                sizeBytes = value(SIZE_BYTES)?.toLong(),
                width = value(WIDTH)?.toInt(),
                height = value(HEIGHT)?.toInt(),
                placeholder = value(PLACEHOLDER),
                alt = null,
            ),
            fileName = value(FILE_NAME),
            text = value(TEXT),
            replyTo = value(REPLY_TO),
            key = value(KEY)?.fromHex()?.toDataByteArray(),
            failure = readFailure(properties),
            // An entry kept before the kinds of upload names only whether it seals.
            upload = value(UPLOAD)?.let(ProductAttachmentUpload::valueOf)
                ?: if (value(SEALS).toBoolean()) ProductAttachmentUpload.SEALED else ProductAttachmentUpload.PREIMAGE,
            sealed = value(SEALED)?.let { BinaryScale.decodeFromByteArray<ChatResourceLocal.Sealed>(it.fromHex()).toDomain() },
            hopFile = value(HOP_FILE)?.let { BinaryScale.decodeFromByteArray<ChatResourceLocal.Hop>(it.fromHex()).toDomain() },
        )
    }

    private fun writeFailure(properties: Properties, failure: ProductAttachmentFailure) {
        val (kind, detail) = when (failure) {
            is ProductAttachmentFailure.TooLarge -> FAILURE_TOO_LARGE to failure.maxBytes.toString()
            ProductAttachmentFailure.NotDeclared -> FAILURE_NOT_DECLARED to null
            ProductAttachmentFailure.PermissionDenied -> FAILURE_PERMISSION_DENIED to null
            is ProductAttachmentFailure.AllowanceUnavailable -> FAILURE_ALLOWANCE to failure.reason
            is ProductAttachmentFailure.Failed -> FAILURE_FAILED to failure.reason
            is ProductAttachmentFailure.Refused -> FAILURE_REFUSED to failure.reason
        }
        properties[FAILURE] = kind
        detail?.let { properties[FAILURE_DETAIL] = it }
    }

    private fun readFailure(properties: Properties): ProductAttachmentFailure? {
        val detail = properties.getProperty(FAILURE_DETAIL)
        return when (properties.getProperty(FAILURE)) {
            null -> null
            FAILURE_TOO_LARGE -> ProductAttachmentFailure.TooLarge(detail?.toLongOrNull() ?: 0)
            FAILURE_NOT_DECLARED -> ProductAttachmentFailure.NotDeclared
            FAILURE_PERMISSION_DENIED -> ProductAttachmentFailure.PermissionDenied
            FAILURE_ALLOWANCE -> ProductAttachmentFailure.AllowanceUnavailable(detail.orEmpty())
            FAILURE_REFUSED -> ProductAttachmentFailure.Refused(detail)
            else -> ProductAttachmentFailure.Failed(detail.orEmpty())
        }
    }

    companion object {
        /** The message id of an entry starts with this prefix. No message of a product or of the host has it. */
        const val ID_PREFIX = "outbox:"

        fun isOutboxId(messageId: ChatMessageId): Boolean = messageId.startsWith(ID_PREFIX)

        private const val FILE_SCHEME = "file://"
        private const val ENTRY_FILE = "entry.properties"
        private const val BYTES_FILE = "bytes"

        private const val ID = "id"
        private const val CHAT_ID = "chatId"
        private const val CREATED_AT = "createdAt"
        private const val KIND = "kind"
        private const val MIME_TYPE = "mimeType"
        private const val SIZE_BYTES = "sizeBytes"
        private const val WIDTH = "width"
        private const val HEIGHT = "height"
        private const val PLACEHOLDER = "placeholder"
        private const val FILE_NAME = "fileName"
        private const val TEXT = "text"
        private const val REPLY_TO = "replyTo"
        private const val KEY = "key"
        private const val SEALS = "seals"
        private const val UPLOAD = "upload"
        private const val HOP_FILE = "hop_file"
        private const val SEALED = "sealed"
        private const val FAILURE = "failure"
        private const val FAILURE_DETAIL = "failureDetail"

        private const val FAILURE_TOO_LARGE = "TooLarge"
        private const val FAILURE_NOT_DECLARED = "NotDeclared"
        private const val FAILURE_PERMISSION_DENIED = "PermissionDenied"
        private const val FAILURE_ALLOWANCE = "AllowanceUnavailable"
        private const val FAILURE_FAILED = "Failed"
        private const val FAILURE_REFUSED = "Refused"
    }
}
