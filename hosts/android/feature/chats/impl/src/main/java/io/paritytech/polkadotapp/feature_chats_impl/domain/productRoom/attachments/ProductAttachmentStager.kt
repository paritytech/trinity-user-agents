package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
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
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentFailure
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/** Runs the staging of one outbox entry in the background, so that it continues after the room screen closes. */
interface ProductAttachmentStageScheduler {
    fun schedule(entryId: ChatMessageId)

    fun cancel(entryId: ChatMessageId)
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

/** A failed entry of the outbox, for the room screen that shows the reason. */
data class ProductAttachmentFailureEvent(val chatId: ChatId, val entryId: ChatMessageId, val failure: ProductAttachmentFailure)

/**
 * Sends the attachments of the outbox (07-LATER §7). The core stages the bytes as one preimage that the product pays
 * for, or, in a room that declares `sealed_attachments`, seals them as encrypted Bulletin parts. The stager then sends
 * the message with the resource: `user_action` `NewMessage` in a
 * product-source room, a stored message and `MessagePosted` in a host-store room. A failure keeps the entry with its
 * reason. Retry stages again, or only sends the message when the key exists. Delete removes the entry and its bytes.
 */
@Singleton
class ProductAttachmentStager @Inject constructor(
    private val outbox: ProductAttachmentOutbox,
    private val staging: ProductAttachmentStaging,
    private val scheduler: ProductAttachmentStageScheduler,
    private val rooms: ProductAttachmentRooms,
    private val feedListeners: ProductFeedListeners,
) {
    private val failures = MutableSharedFlow<ProductAttachmentFailureEvent>(extraBufferCapacity = FAILURE_BUFFER)

    fun observe(chatId: ChatId): Flow<List<ProductOutboxEntry>> = outbox.observe(chatId)

    /** The kept bytes of an entry, which the bubble draws until the host sends the attachment. */
    fun preview(entry: ProductOutboxEntry): ChatResourceModel = ChatResourceModel.Url(outbox.previewUrl(entry.id))

    /** The failures of the entries of a room, as they occur. */
    fun observeFailures(chatId: ChatId): Flow<ProductAttachmentFailureEvent> = failures.filter { it.chatId == chatId }

    suspend fun enqueue(
        chatId: ChatId,
        draft: ProductAttachmentDraft,
        text: String?,
        replyTo: ChatMessageId?,
        seals: Boolean = false,
    ): ProductOutboxEntry {
        val entry = outbox.add(chatId, draft, text, replyTo, seals)
        scheduler.schedule(entry.id)
        return entry
    }

    /** Sends a failed entry again. False when the entry is gone or did not fail. */
    suspend fun retry(entryId: ChatMessageId): Boolean {
        val entry = outbox.get(entryId)?.takeIf { it.isFailed } ?: return false
        outbox.update(entry.id) { it.copy(failure = null) }
        scheduler.schedule(entry.id)
        return true
    }

    suspend fun delete(entryId: ChatMessageId) {
        scheduler.cancel(entryId)
        outbox.remove(entryId)
    }

    /** Stages one entry and sends its message. The background job calls it. */
    suspend fun process(entryId: ChatMessageId) {
        val entry = outbox.get(entryId)?.takeIf { !it.isFailed } ?: return

        try {
            staging.holdingWorker(entry.chatId) { send(entry) }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Timber.w(error, "truapi.chat.stage %s failed", entry.id)
            fail(entry, ProductAttachmentFailure.Failed(error.message ?: error.toString()))
        }
    }

    private suspend fun send(entry: ProductOutboxEntry) {
        val (staged, resource) = if (entry.seals) {
            val sealed = entry.sealed ?: seal(entry) ?: return
            (outbox.update(entry.id) { it.copy(sealed = sealed) } ?: return) to sealed
        } else {
            val key = entry.key ?: stage(entry) ?: return
            (outbox.update(entry.id) { it.copy(key = key) } ?: return) to ChatResourceModel.Preimage(key)
        }

        deliver(staged, resource)
            .onSuccess {
                outbox.remove(staged.id)
                Timber.i("truapi.chat.stage %s sent", staged.id)
            }
            .onFailure { error ->
                if (error is CancellationException) throw error
                Timber.w(error, "truapi.chat.stage %s not sent", staged.id)
                fail(staged, error.toDeliveryFailure())
            }
    }

    private suspend fun stage(entry: ProductOutboxEntry): DataByteArray? =
        upload(entry, "stage") { bytes -> staging.stage(entry.chatId, entry.meta.mimeType, bytes) }

    private suspend fun seal(entry: ProductOutboxEntry): ChatResourceModel.Sealed? =
        upload(entry, "seal") { bytes -> staging.seal(entry.chatId, bytes) }

    private suspend fun <T> upload(entry: ProductOutboxEntry, step: String, upload: suspend (ByteArray) -> Result<T>): T? {
        val bytes = outbox.bytes(entry.id) ?: run {
            fail(entry, ProductAttachmentFailure.Failed("the bytes of the attachment are gone"))
            return null
        }

        val started = System.nanoTime()
        val outcome = upload(bytes)
        val elapsedMs = (System.nanoTime() - started) / NANOS_IN_MILLI
        // The emulator launcher reads this line for the upload time of B7.
        Timber.i("truapi.chat.%s %s %d B in %d ms: %s", step, entry.id, bytes.size, elapsedMs, outcome.outcomeName())

        return outcome.getOrElse { error ->
            if (error is CancellationException) throw error
            fail(entry, error.toStageFailure())
            null
        }
    }

    private suspend fun deliver(entry: ProductOutboxEntry, resource: ChatResourceModel): Result<Unit> {
        val policy = rooms.policy(entry.chatId) ?: return Result.failure(IllegalStateException("the room is gone"))
        val content = entry.content(resource)

        if (policy.messageStore == ChatRoomPolicy.MessageStore.HOST) {
            return runCatching { rooms.saveUserMessage(entry.chatId, content, entry.replyTo) }
        }

        val querying = rooms.querying(entry.chatId) ?: return Result.failure(IllegalStateException("no product answers the room"))
        return querying.userAction(entry.chatId, ChatUserActionModel.NewMessage(content, entry.replyTo)).map { outcome ->
            // An open room shows the message that the product made in place of the entry.
            if (outcome is ChatUserActionOutcome.MessageCreated) {
                feedListeners.deliver(entry.chatId, ProductFeedUpdateModel.New(outcome.message))
            }
        }
    }

    private suspend fun fail(entry: ProductOutboxEntry, failure: ProductAttachmentFailure) {
        outbox.update(entry.id) { it.copy(failure = failure) } ?: return
        failures.emit(ProductAttachmentFailureEvent(entry.chatId, entry.id, failure))
    }

    private fun Result<*>.outcomeName(): String =
        exceptionOrNull()?.let { "${it::class.simpleName ?: "Failure"} (${it.message})" } ?: "Staged"

    private fun Throwable.toStageFailure(): ProductAttachmentFailure = when (this) {
        is ProductAttachmentStageException.TooLarge -> ProductAttachmentFailure.TooLarge(maxBytes)
        is ProductAttachmentStageException.NotDeclared -> ProductAttachmentFailure.NotDeclared
        is ProductAttachmentStageException.PermissionDenied -> ProductAttachmentFailure.PermissionDenied
        is ProductAttachmentStageException.AllowanceUnavailable -> ProductAttachmentFailure.AllowanceUnavailable(reason)
        is ProductAttachmentStageException.SubmitFailed -> ProductAttachmentFailure.Failed(reason)
        else -> ProductAttachmentFailure.Failed(message ?: toString())
    }

    private fun Throwable.toDeliveryFailure(): ProductAttachmentFailure = when (this) {
        is ProductChatQueryException.Rejected -> ProductAttachmentFailure.Refused(reason)
        else -> ProductAttachmentFailure.Failed(message ?: toString())
    }

    private companion object {
        const val FAILURE_BUFFER = 16
        const val NANOS_IN_MILLI = 1_000_000
    }
}

/** The message of an entry, with [resource] as the resource of its media item. */
fun ProductOutboxEntry.content(resource: ChatResourceModel): ChatMessage.Content {
    val media = when (kind) {
        ProductAttachmentDraft.Kind.IMAGE -> ChatMediaItemModel.Image(resource, meta)
        ProductAttachmentDraft.Kind.VIDEO -> ChatMediaItemModel.Video(resource, meta)
        ProductAttachmentDraft.Kind.FILE -> ChatMediaItemModel.File(resource, meta, fileName = fileName ?: DEFAULT_FILE_NAME)
    }
    return ChatMessage.Content.ProductBody(ProductMessageBody.RichText(text = text, media = listOf(media)))
}

private const val DEFAULT_FILE_NAME = "file"
