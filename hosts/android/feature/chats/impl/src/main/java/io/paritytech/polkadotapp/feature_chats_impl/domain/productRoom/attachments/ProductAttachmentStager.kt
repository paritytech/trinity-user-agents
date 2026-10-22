package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStageException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStaging
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentFailure
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.adjustReaction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.withEditedText
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/** Runs the sending of the outbox of a room in the background, so that it continues after the room screen closes. */
interface ProductAttachmentStageScheduler {
    /** Sends the entries of the room in order, after a run for the room that is on its way, or at once. */
    suspend fun schedule(chatId: ChatId)
}

/** The parts of the chat engine that the stager uses. */
interface ProductAttachmentRooms {
    suspend fun policy(chatId: ChatId): ChatRoomPolicy?

    suspend fun querying(chatId: ChatId): ProductChatQuerying?

    /** Keeps a message of the user in a host-store room. The product gets it as `MessagePosted`. */
    suspend fun saveUserMessage(chatId: ChatId, content: ChatMessage.Content, replyTo: ChatMessageId?)
}

class EngineProductAttachmentRooms @Inject constructor(
    private val chatEngine: ChatEngine,
    private val messageSender: ChatMessageSender,
    private val policyProvider: ChatRoomPolicyProvider,
) : ProductAttachmentRooms {
    override suspend fun policy(chatId: ChatId): ChatRoomPolicy? = policyProvider.observePolicy(chatId).first()

    override suspend fun querying(chatId: ChatId): ProductChatQuerying? = chatEngine.roomQuerying(chatId)

    override suspend fun saveUserMessage(chatId: ChatId, content: ChatMessage.Content, replyTo: ChatMessageId?) {
        messageSender.sendUserMessage(chatId = chatId, content = content, replyToMessageId = replyTo)
    }
}

/**
 * Sends the outbox of a room as the native chat sends its messages (07-LATER §7): in order, each under the UUID that the
 * host chose, and again under the same id until the product has it. An attachment goes first to a HOP node of the person
 * chat. A product-source room gets `user_action` `NewMessage`, a host-store room a stored message and `MessagePosted`.
 * An edit or a reaction that the user made before the product had the message goes right after it. Only a message that
 * cannot go keeps a failure, as the native chat marks a message it cannot deliver.
 */
@Singleton
class ProductAttachmentStager @Inject constructor(
    private val outbox: ProductAttachmentOutbox,
    private val staging: ProductAttachmentStaging,
    private val scheduler: ProductAttachmentStageScheduler,
    private val rooms: ProductAttachmentRooms,
    private val feedListeners: ProductFeedListeners,
    private val hopCache: ChatHopFileCache,
) {
    private sealed interface Outcome {
        data object Sent : Outcome

        /** The message cannot go. The entry keeps the failure, and the next entry goes. */
        data object Failed : Outcome

        /** The message did not go now. It goes again later, before the entries after it. */
        data object Later : Outcome
    }

    // The bytes that the node holds and the size of each attachment that goes to HOP now, by entry id.
    private val uploads = MutableStateFlow<Map<ChatMessageId, UploadProgress>>(emptyMap())

    /** The bytes that the node holds of an attachment that uploads now, and its size. */
    data class UploadProgress(val sent: Long, val total: Long)

    fun observe(chatId: ChatId): Flow<List<ProductOutboxEntry>> = outbox.observe(chatId)

    /** The progress of each attachment that uploads now, by entry id, for the native upload ring. */
    fun observeUploads(): Flow<Map<ChatMessageId, UploadProgress>> = uploads

    /** The newest entry of each room, which the chat list shows as the native list shows a message it sends. */
    fun observeNewest(): Flow<Map<ChatId, ProductOutboxEntry>> = outbox.observeAll().map { entries -> entries.associateBy { it.chatId } }

    /** The kept bytes of an entry, which the bubble draws until the host sends the attachment. */
    fun preview(entry: ProductOutboxEntry): ChatResourceModel = ChatResourceModel.Url(outbox.previewUrl(entry.id))

    /** True for the resource of an attachment that waits in the outbox. */
    fun isOutboxPreview(resource: ChatResourceModel): Boolean = resource is ChatResourceModel.Url && outbox.file(resource.url) != null

    suspend fun enqueue(
        chatId: ChatId,
        draft: ProductAttachmentDraft,
        text: String?,
        replyTo: ChatMessageId?,
    ): ProductOutboxEntry {
        val entry = outbox.add(chatId, draft, text, replyTo)
        scheduler.schedule(chatId)
        return entry
    }

    /** [request]: the text is the welcome message of a chat request. */
    suspend fun enqueueText(chatId: ChatId, text: String, replyTo: ChatMessageId?, request: Boolean): ProductOutboxEntry {
        val entry = outbox.addText(chatId, text, replyTo, request)
        scheduler.schedule(chatId)
        return entry
    }

    /** Sends the entries of a room again, for example when its room screen opens. */
    suspend fun resume(chatId: ChatId) {
        if (outbox.observe(chatId).first().any { !it.isFailed }) scheduler.schedule(chatId)
    }

    /** True when an entry of the room still has bytes to upload, so its run shows the upload notification. */
    suspend fun hasUploads(chatId: ChatId): Boolean = outbox.observe(chatId).first().any { !it.isText && it.hopFile == null && !it.isFailed }

    /** The message with this id while it waits in the outbox. */
    suspend fun pending(entryId: ChatMessageId): ProductOutboxEntry? = outbox.get(entryId)

    /** Removes an entry and its bytes. False when no entry has the id. */
    suspend fun delete(entryId: ChatMessageId): Boolean = outbox.remove(entryId)

    /** The edit of a message that waits in the outbox. It goes after the message. False when no entry has the id. */
    suspend fun editPending(entryId: ChatMessageId, text: String, at: Long): Boolean =
        outbox.update(entryId) { it.copy(editedText = text, editedAt = at) } != null

    /**
     * Adds the reaction of the user to a message that waits in the outbox, or removes it when the user has it. As in a
     * person chat, the user has one reaction a message. False when no entry has the id.
     */
    suspend fun toggleReactionPending(entryId: ChatMessageId, emoji: String): Boolean =
        outbox.update(entryId) { it.copy(reaction = if (it.reaction == emoji) null else emoji) } != null

    /** Sends the entries of a room, the oldest first. False when an entry has to go again later. */
    suspend fun processRoom(chatId: ChatId): Boolean {
        for (entry in outbox.observe(chatId).first()) {
            if (entry.isFailed) continue
            if (process(entry) == Outcome.Later) return false
        }
        return true
    }

    private suspend fun process(entry: ProductOutboxEntry): Outcome = try {
        staging.holdingWorker(entry.chatId) { send(entry) }
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        Timber.w(error, "truapi.chat.stage %s goes again later", entry.id)
        Outcome.Later
    }

    private suspend fun send(entry: ProductOutboxEntry): Outcome {
        val staged = if (entry.isText || entry.hopFile != null) {
            entry
        } else {
            val hopFile = when (val upload = uploadToHop(entry)) {
                is UploadOutcome.Uploaded -> upload.hopFile
                UploadOutcome.Failed -> return Outcome.Failed
                UploadOutcome.Later -> return Outcome.Later
            }
            outbox.update(entry.id) { it.copy(hopFile = hopFile) } ?: return Outcome.Sent
        }

        val policy = rooms.policy(staged.chatId) ?: return Outcome.Later
        if (policy.messageStore == ChatRoomPolicy.MessageStore.HOST) {
            rooms.saveUserMessage(staged.chatId, staged.content(staged.hopFile), staged.replyTo)
            outbox.remove(staged.id)
            return Outcome.Sent
        }

        val querying = rooms.querying(staged.chatId) ?: return Outcome.Later
        val action = ChatUserActionModel.NewMessage(
            staged.content(staged.hopFile),
            staged.replyTo,
            messageId = staged.id,
            timestamp = staged.createdAt,
        )
        val result = querying.userAction(staged.chatId, action)
        val error = result.exceptionOrNull()
        if (error != null) return deliveryFailed(staged, error)

        // An open room shows the message that the product made, with what the user did to it in the meantime.
        (result.getOrNull() as? ChatUserActionOutcome.MessageCreated)?.let { created ->
            val current = outbox.get(staged.id) ?: staged
            feedListeners.deliver(staged.chatId, ProductFeedUpdateModel.New(created.message.withPendingActions(current)))
        }

        return sendPendingActions(staged.id, querying)
    }

    /** Sends the edit and the reaction that wait for the message, then removes the entry once nothing more waits. */
    private suspend fun sendPendingActions(entryId: ChatMessageId, querying: ProductChatQuerying): Outcome {
        while (true) {
            val entry = outbox.get(entryId) ?: return Outcome.Sent

            entry.editedText?.let { text ->
                val edit = ChatUserActionModel.EditMessage(entry.id, ChatMessage.Content.Text(text))
                if (!sendFollowUp(entry, querying, edit)) return Outcome.Later
                outbox.update(entry.id) { if (it.editedText == text) it.copy(editedText = null) else it }
                continue
            }

            entry.reaction?.let { emoji ->
                if (!sendFollowUp(entry, querying, ChatUserActionModel.ReactionPlaced(entry.id, emoji))) return Outcome.Later
                outbox.update(entry.id) { if (it.reaction == emoji) it.copy(reaction = null) else it }
                continue
            }

            if (outbox.removeIfUnchanged(entry)) {
                Timber.i("truapi.chat.stage %s sent", entry.id)
                return Outcome.Sent
            }
        }
    }

    /** False when the action has to go again later. A refused action does not go again. */
    private suspend fun sendFollowUp(entry: ProductOutboxEntry, querying: ProductChatQuerying, action: ChatUserActionModel): Boolean {
        val error = querying.userAction(entry.chatId, action).exceptionOrNull() ?: return true
        if (error is CancellationException) throw error
        Timber.w(error, "truapi.chat.stage %s: an action on the message did not go", entry.id)
        return error is ProductChatQueryException.Rejected || error is ProductChatQueryException.UnknownMessage
    }

    private suspend fun deliveryFailed(entry: ProductOutboxEntry, error: Throwable): Outcome {
        if (error is CancellationException) throw error
        Timber.w(error, "truapi.chat.stage %s not sent", entry.id)

        return when (error) {
            // The product does not take the message, so it cannot go, as a native message that is too large.
            is ProductChatQueryException.Rejected -> {
                fail(entry, ProductAttachmentFailure.Refused(error.reason))
                Outcome.Failed
            }

            // The room is gone, and its messages with it.
            is ProductChatQueryException.UnknownRoom -> {
                outbox.remove(entry.id)
                Outcome.Sent
            }

            // The product answered with an error. A few times it can be passing; past that the message cannot go, and
            // the messages after it go.
            is ProductChatQueryException.Failed,
            is ProductChatQueryException.UnknownMessage,
            is ProductChatQueryException.NotAvailable -> {
                val counted = outbox.update(entry.id) { it.copy(refusals = it.refusals + 1) }
                if ((counted?.refusals ?: 0) < MAX_REFUSALS) return Outcome.Later
                fail(entry, ProductAttachmentFailure.Failed(error.message.orEmpty()))
                Outcome.Failed
            }

            // The worker does not run or did not answer in time: the message goes again, as a native message does.
            else -> Outcome.Later
        }
    }

    private sealed interface UploadOutcome {
        data class Uploaded(val hopFile: ChatResourceModel.Hop) : UploadOutcome

        data object Failed : UploadOutcome

        data object Later : UploadOutcome
    }

    private suspend fun uploadToHop(entry: ProductOutboxEntry): UploadOutcome {
        val bytes = outbox.bytes(entry.id) ?: run {
            fail(entry, ProductAttachmentFailure.Failed("the bytes of the attachment are gone"))
            return UploadOutcome.Failed
        }

        val started = System.nanoTime()
        val outcome = try {
            staging.uploadToHop(entry.chatId, bytes) { sent, total ->
                uploads.update { it + (entry.id to UploadProgress(sent, total)) }
            }
        } finally {
            uploads.update { it - entry.id }
        }
        val elapsedMs = (System.nanoTime() - started) / NANOS_IN_MILLI
        // The emulator launcher reads this line for the upload time of B7.
        Timber.i("truapi.chat.hop %s %d B in %d ms: %s", entry.id, bytes.size, elapsedMs, outcome.outcomeName())

        val hopFile = outcome.getOrElse { error ->
            if (error is CancellationException) throw error
            return when (error) {
                // As the native upload, a failed upload goes again; only an attachment the room cannot take fails.
                is ProductAttachmentStageException.TooLarge -> {
                    fail(entry, ProductAttachmentFailure.TooLarge(error.maxBytes))
                    UploadOutcome.Failed
                }

                is ProductAttachmentStageException.NotDeclared -> {
                    fail(entry, ProductAttachmentFailure.NotDeclared)
                    UploadOutcome.Failed
                }

                else -> UploadOutcome.Later
            }
        }

        // This device shows its own photo from these bytes, and so never opens and acks the file it sent.
        entry.meta?.let { meta ->
            hopCache.keepUpload(hopFile, meta.mimeType, bytes).onFailure { Timber.w(it, "truapi.chat.hop %s not kept", entry.id) }
        }
        return UploadOutcome.Uploaded(hopFile)
    }

    private suspend fun fail(entry: ProductOutboxEntry, failure: ProductAttachmentFailure) {
        outbox.update(entry.id) { it.copy(failure = failure) }
    }

    private fun Result<*>.outcomeName(): String =
        exceptionOrNull()?.let { "${it::class.simpleName ?: "Failure"} (${it.message})" } ?: "Staged"

    private companion object {
        const val NANOS_IN_MILLI = 1_000_000
        const val MAX_REFUSALS = 5
    }
}

/** The message of an entry. [resource] is the resource of its media item; a text entry has none. */
fun ProductOutboxEntry.content(resource: ChatResourceModel?): ChatMessage.Content {
    val kind = kind ?: return ChatMessage.Content.Text(text.orEmpty())
    val meta = requireNotNull(meta) { "An attachment of the outbox has its meta" }
    val item = requireNotNull(resource) { "An attachment of the outbox has a resource" }
    val media = when (kind) {
        ProductAttachmentDraft.Kind.IMAGE -> ChatMediaItemModel.Image(item, meta)
        ProductAttachmentDraft.Kind.VIDEO -> ChatMediaItemModel.Video(item, meta)
        ProductAttachmentDraft.Kind.FILE -> ChatMediaItemModel.File(item, meta, fileName = fileName ?: DEFAULT_FILE_NAME)
    }
    return ChatMessage.Content.ProductBody(ProductMessageBody.RichText(text = text, media = listOf(media)))
}

/** The message as the user sees it while it waits: with the edit and the reaction that wait for it. */
internal fun ProductRoomMessage.withPendingActions(entry: ProductOutboxEntry): ProductRoomMessage {
    val edited = entry.editedText?.let { text -> copy(content = content.withEditedText(text), editedAt = entry.editedAt) } ?: this
    return entry.reaction?.let { edited.adjustReaction(it, ProductParticipant.USER, placed = true) } ?: edited
}

private const val DEFAULT_FILE_NAME = "file"
