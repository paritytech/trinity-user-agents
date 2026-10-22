package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
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

/** An attachment that the host uploads to HOP, with the meta of its media item. */
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
    /** The core does not upload an attachment of this size. */
    data class TooLarge(val maxBytes: Long) : ProductAttachmentFailure

    data object NotDeclared : ProductAttachmentFailure

    /** The upload or the delivery to the product failed. A retry can succeed. */
    data class Failed(val reason: String) : ProductAttachmentFailure

    /** The product refused the message. [reason] is for the user. */
    data class Refused(val reason: String?) : ProductAttachmentFailure
}

/** Gives the kept file of an outbox entry for its preview address, so the room draws the attachment before it is sent. */
fun interface ChatOutboxPreviews {
    /** Null for an address that names no kept file of the outbox. */
    fun file(url: String): File?
}

/**
 * A message that the user sent and that waits in the outbox until the product has it: a text, or an attachment that
 * the host uploads first. The native chat keeps a message it sends the same way and sends it again until it goes.
 */
data class ProductOutboxEntry(
    /** A UUID that the host chose, as the native chat names its messages. The product sends the message under this id. */
    val id: ChatMessageId,
    val chatId: ChatId,
    /** Milliseconds since the Unix epoch. */
    val createdAt: Long,
    /** Null for a text message. */
    val kind: ProductAttachmentDraft.Kind?,
    /** Null for a text message. */
    val meta: ChatMediaMetaModel?,
    val fileName: String?,
    /** The text of a text message, or the caption of an attachment. */
    val text: String?,
    val replyTo: ChatMessageId?,
    /** The message is the welcome message of a chat request. */
    val request: Boolean = false,
    /** Null while the host sends the message. Set only when the message cannot go. */
    val failure: ProductAttachmentFailure?,
    /** The HOP file, after the upload. Sending again then only sends the message. */
    val hopFile: ChatResourceModel.Hop?,
    /** The edit that the user made before the product had the message. The host sends it after the message. */
    val editedText: String? = null,
    val editedAt: Long? = null,
    /** The reaction that the user put on the message before the product had it. The host sends it after the message. */
    val reaction: String? = null,
    /** The times the product answered the message with an error. Past a limit the message fails, so it blocks no other. */
    val refusals: Int = 0,
) {
    val isFailed: Boolean
        get() = failure != null

    val isText: Boolean
        get() = kind == null
}

/**
 * Keeps the messages that the user sends to a product-source room, and the attachments that the host uploads, until
 * the host sends them (07-LATER §7, D27). Each entry is a folder with a properties file and, for an attachment, its
 * bytes, so an entry stays after the room screen closes and after the process stops.
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

    /** The entries of every room, the oldest first. */
    fun observeAll(): Flow<List<ProductOutboxEntry>> = entries
        .onStart { loaded() }
        .filterNotNull()
        .map { all -> all.values.sortedBy { it.createdAt } }
        .distinctUntilChanged()

    suspend fun all(): List<ProductOutboxEntry> = loaded().values.sortedBy { it.createdAt }

    suspend fun get(id: ChatMessageId): ProductOutboxEntry? = loaded()[id]

    suspend fun add(
        chatId: ChatId,
        draft: ProductAttachmentDraft,
        text: String?,
        replyTo: ChatMessageId?,
    ): ProductOutboxEntry =
        mutex.withLock {
            val entry = ProductOutboxEntry(
                id = UUID.randomUUID().toString(),
                chatId = chatId,
                createdAt = now(),
                kind = draft.kind,
                meta = draft.meta,
                fileName = draft.fileName,
                text = text,
                replyTo = replyTo,
                failure = null,
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

    /** Adds a text message of the user. */
    suspend fun addText(chatId: ChatId, text: String, replyTo: ChatMessageId?, request: Boolean): ProductOutboxEntry =
        mutex.withLock {
            val entry = ProductOutboxEntry(
                id = UUID.randomUUID().toString(),
                chatId = chatId,
                createdAt = now(),
                kind = null,
                meta = null,
                fileName = null,
                text = text,
                replyTo = replyTo,
                request = request,
                failure = null,
                hopFile = null,
            )
            withContext(io) { write(entry) }
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

    /** Removes an entry and its bytes. False when the entry was gone. */
    suspend fun remove(id: ChatMessageId): Boolean = mutex.withLock {
        val current = loadedLocked()
        withContext(io) { folderOf(id).deleteRecursively() }
        publish(current - id)
        id in current
    }

    /** Removes an entry that is still [expected], with its bytes. False when the entry changed or is gone. */
    suspend fun removeIfUnchanged(expected: ProductOutboxEntry): Boolean = mutex.withLock {
        val current = loadedLocked()
        if (current[expected.id] != expected) return@withLock false
        withContext(io) { folderOf(expected.id).deleteRecursively() }
        publish(current - expected.id)
        true
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

    private fun folderOf(id: ChatMessageId): File = File(root, id.removePrefix(LEGACY_ID_PREFIX))

    private fun write(entry: ProductOutboxEntry) {
        val properties = Properties()
        properties[ID] = entry.id
        properties[CHAT_ID] = entry.chatId.value.value.toHexString()
        properties[CREATED_AT] = entry.createdAt.toString()
        entry.kind?.let { properties[KIND] = it.name }
        entry.meta?.let { meta ->
            properties[MIME_TYPE] = meta.mimeType
            meta.sizeBytes?.let { properties[SIZE_BYTES] = it.toString() }
            meta.width?.let { properties[WIDTH] = it.toString() }
            meta.height?.let { properties[HEIGHT] = it.toString() }
            meta.placeholder?.let { properties[PLACEHOLDER] = it }
            meta.durationMs?.let { properties[DURATION_MS] = it.toString() }
        }
        entry.fileName?.let { properties[FILE_NAME] = it }
        entry.text?.let { properties[TEXT] = it }
        entry.replyTo?.let { properties[REPLY_TO] = it }
        if (entry.request) properties[REQUEST] = true.toString()
        entry.editedText?.let { properties[EDITED_TEXT] = it }
        entry.editedAt?.let { properties[EDITED_AT] = it.toString() }
        entry.reaction?.let { properties[REACTION] = it }
        if (entry.refusals > 0) properties[REFUSALS] = entry.refusals.toString()
        properties[UPLOAD] = if (entry.isText) UPLOAD_NONE else UPLOAD_HOP
        entry.hopFile?.let { properties[HOP_FILE] = BinaryScale.encodeToByteArray(it.toLocal()).toHexString() }
        entry.failure?.let { failure -> writeFailure(properties, failure) }

        val folder = folderOf(entry.id).apply { mkdirs() }
        val temporary = File(folder, "$ENTRY_FILE.tmp")
        temporary.outputStream().use { properties.store(it, null) }
        check(temporary.renameTo(File(folder, ENTRY_FILE))) { "The outbox entry ${entry.id} cannot be written" }
    }

    private fun read(folder: File): ProductOutboxEntry? {
        val file = File(folder, ENTRY_FILE)
        if (!file.isFile) return null
        val properties = Properties().apply { file.inputStream().use(::load) }
        fun value(name: String): String? = properties.getProperty(name)
        // An entry that an earlier build kept for a preimage or a sealed file has no way to go any more.
        val isText = when (value(UPLOAD)) {
            UPLOAD_NONE -> true
            UPLOAD_HOP -> false
            else -> return null
        }
        if (!isText && !File(folder, BYTES_FILE).isFile) return null

        return ProductOutboxEntry(
            // An earlier build named an entry with a prefix that no product message id may carry.
            id = requireNotNull(value(ID)).removePrefix(LEGACY_ID_PREFIX),
            chatId = ChatId.fromRawValue(requireNotNull(value(CHAT_ID)).fromHex()),
            createdAt = requireNotNull(value(CREATED_AT)).toLong(),
            kind = if (isText) null else ProductAttachmentDraft.Kind.valueOf(requireNotNull(value(KIND))),
            meta = if (isText) {
                null
            } else {
                ChatMediaMetaModel(
                    mimeType = requireNotNull(value(MIME_TYPE)),
                    sizeBytes = value(SIZE_BYTES)?.toLong(),
                    width = value(WIDTH)?.toInt(),
                    height = value(HEIGHT)?.toInt(),
                    placeholder = value(PLACEHOLDER),
                    alt = null,
                    durationMs = value(DURATION_MS)?.toLong(),
                )
            },
            fileName = value(FILE_NAME),
            text = value(TEXT),
            replyTo = value(REPLY_TO),
            request = value(REQUEST).toBoolean(),
            failure = readFailure(properties),
            hopFile = value(HOP_FILE)?.let { BinaryScale.decodeFromByteArray<ChatResourceLocal.Hop>(it.fromHex()).toDomain() },
            editedText = value(EDITED_TEXT),
            editedAt = value(EDITED_AT)?.toLong(),
            reaction = value(REACTION),
            refusals = value(REFUSALS)?.toIntOrNull() ?: 0,
        )
    }

    private fun writeFailure(properties: Properties, failure: ProductAttachmentFailure) {
        val (kind, detail) = when (failure) {
            is ProductAttachmentFailure.TooLarge -> FAILURE_TOO_LARGE to failure.maxBytes.toString()
            ProductAttachmentFailure.NotDeclared -> FAILURE_NOT_DECLARED to null
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
            FAILURE_REFUSED -> ProductAttachmentFailure.Refused(detail)
            else -> ProductAttachmentFailure.Failed(detail.orEmpty())
        }
    }

    companion object {
        /** The prefix of the entry ids of an earlier build. A read entry drops it. */
        private const val LEGACY_ID_PREFIX = "outbox:"

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
        private const val DURATION_MS = "durationMs"
        private const val FILE_NAME = "fileName"
        private const val TEXT = "text"
        private const val REPLY_TO = "replyTo"
        private const val REQUEST = "request"
        private const val EDITED_TEXT = "editedText"
        private const val EDITED_AT = "editedAt"
        private const val REACTION = "reaction"
        private const val REFUSALS = "refusals"
        private const val HOP_FILE = "hop_file"
        private const val UPLOAD = "upload"
        private const val UPLOAD_HOP = "HOP"
        private const val UPLOAD_NONE = "NONE"
        private const val FAILURE = "failure"
        private const val FAILURE_DETAIL = "failureDetail"

        private const val FAILURE_TOO_LARGE = "TooLarge"
        private const val FAILURE_NOT_DECLARED = "NotDeclared"
        private const val FAILURE_FAILED = "Failed"
        private const val FAILURE_REFUSED = "Refused"
    }
}
