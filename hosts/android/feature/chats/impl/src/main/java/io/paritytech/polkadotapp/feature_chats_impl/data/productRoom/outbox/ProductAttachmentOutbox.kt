package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
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
import timber.log.Timber
import java.io.File
import java.util.Properties
import java.util.UUID

/** Why a message of the outbox was not sent. */
sealed interface ProductAttachmentFailure {
    /** The delivery to the product failed. A retry can succeed. */
    data class Failed(val reason: String) : ProductAttachmentFailure

    /** The product refused the message. [reason] is for the user. */
    data class Refused(val reason: String?) : ProductAttachmentFailure
}

/**
 * A text message that the user sent and that waits in the outbox until the product has it. The host sends it again
 * until it goes.
 */
data class ProductOutboxEntry(
    /** A UUID that the host chose. The product sends the message under this id. */
    val id: ChatMessageId,
    val chatId: ChatId,
    /** Milliseconds since the Unix epoch. */
    val createdAt: Long,
    val text: String?,
    val replyTo: ChatMessageId?,
    /** Null while the host sends the message. Set only when the message cannot go. */
    val failure: ProductAttachmentFailure?,
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
}

/**
 * Keeps the messages that the user sends to a product-source room until the host sends them. Each entry is a folder
 * with a properties file, so an entry stays after the room screen closes and after the process stops.
 */
class ProductAttachmentOutbox(
    private val root: File,
    private val io: CoroutineDispatcher,
    private val now: () -> Long,
) {
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

    /** Adds a text message of the user. */
    suspend fun addText(chatId: ChatId, text: String, replyTo: ChatMessageId?): ProductOutboxEntry =
        mutex.withLock {
            val entry = ProductOutboxEntry(
                id = UUID.randomUUID().toString(),
                chatId = chatId,
                createdAt = now(),
                text = text,
                replyTo = replyTo,
                failure = null,
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

    /** Removes an entry. False when the entry was gone. */
    suspend fun remove(id: ChatMessageId): Boolean = mutex.withLock {
        val current = loadedLocked()
        withContext(io) { folderOf(id).deleteRecursively() }
        publish(current - id)
        id in current
    }

    /** Removes an entry that is still [expected]. False when the entry changed or is gone. */
    suspend fun removeIfUnchanged(expected: ProductOutboxEntry): Boolean = mutex.withLock {
        val current = loadedLocked()
        if (current[expected.id] != expected) return@withLock false
        withContext(io) { folderOf(expected.id).deleteRecursively() }
        publish(current - expected.id)
        true
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
                .onFailure { Timber.w(it, "An entry of the outbox cannot be read; it is removed") }
                .getOrNull()
                ?: run {
                    folder.deleteRecursively()
                    null
                }
        }.associateBy { it.id }
    }

    private fun folderOf(id: ChatMessageId): File = File(root, id)

    private fun write(entry: ProductOutboxEntry) {
        val properties = Properties()
        properties[ID] = entry.id
        properties[CHAT_ID] = entry.chatId.value.value.toHexString()
        properties[CREATED_AT] = entry.createdAt.toString()
        entry.text?.let { properties[TEXT] = it }
        entry.replyTo?.let { properties[REPLY_TO] = it }
        entry.editedText?.let { properties[EDITED_TEXT] = it }
        entry.editedAt?.let { properties[EDITED_AT] = it.toString() }
        entry.reaction?.let { properties[REACTION] = it }
        if (entry.refusals > 0) properties[REFUSALS] = entry.refusals.toString()
        properties[UPLOAD] = UPLOAD_NONE
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
        // The outbox keeps text messages only. An entry of another kind has no way to go.
        if (value(UPLOAD) != UPLOAD_NONE) return null

        return ProductOutboxEntry(
            id = requireNotNull(value(ID)),
            chatId = ChatId.fromRawValue(requireNotNull(value(CHAT_ID)).fromHex()),
            createdAt = requireNotNull(value(CREATED_AT)).toLong(),
            text = value(TEXT),
            replyTo = value(REPLY_TO),
            failure = readFailure(properties),
            editedText = value(EDITED_TEXT),
            editedAt = value(EDITED_AT)?.toLong(),
            reaction = value(REACTION),
            refusals = value(REFUSALS)?.toIntOrNull() ?: 0,
        )
    }

    private fun writeFailure(properties: Properties, failure: ProductAttachmentFailure) {
        val (kind, detail) = when (failure) {
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
            FAILURE_REFUSED -> ProductAttachmentFailure.Refused(detail)
            else -> ProductAttachmentFailure.Failed(detail.orEmpty())
        }
    }

    companion object {
        private const val ENTRY_FILE = "entry.properties"

        private const val ID = "id"
        private const val CHAT_ID = "chatId"
        private const val CREATED_AT = "createdAt"
        private const val TEXT = "text"
        private const val REPLY_TO = "replyTo"
        private const val EDITED_TEXT = "editedText"
        private const val EDITED_AT = "editedAt"
        private const val REACTION = "reaction"
        private const val REFUSALS = "refusals"
        private const val UPLOAD = "upload"
        private const val UPLOAD_NONE = "NONE"
        private const val FAILURE = "failure"
        private const val FAILURE_DETAIL = "failureDetail"

        private const val FAILURE_FAILED = "Failed"
        private const val FAILURE_REFUSED = "Refused"
    }
}
