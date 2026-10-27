package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentFailure
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.adjustReaction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.withEditedText
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
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
 * Sends the outbox of a room in order, each message under the UUID that the host chose, and again under the same id
 * until the product has it. A product-source room gets `user_action` `NewMessage`. A room that the product moved to the
 * host store meanwhile gets a stored message and `MessagePosted`. An edit or a reaction that the user made before the
 * product had the message goes right after it. Only a message that cannot go keeps a failure.
 */
@Singleton
class ProductAttachmentStager @Inject constructor(
    private val outbox: ProductAttachmentOutbox,
    private val scheduler: ProductAttachmentStageScheduler,
    private val rooms: ProductAttachmentRooms,
    private val feedListeners: ProductFeedListeners,
) {
    private sealed interface Outcome {
        data object Sent : Outcome

        /** The message cannot go. The entry keeps the failure, and the next entry goes. */
        data object Failed : Outcome

        /** The message did not go now. It goes again later, before the entries after it. */
        data object Later : Outcome
    }

    fun observe(chatId: ChatId): Flow<List<ProductOutboxEntry>> = outbox.observe(chatId)

    suspend fun enqueueText(chatId: ChatId, text: String, replyTo: ChatMessageId?): ProductOutboxEntry {
        val entry = outbox.addText(chatId, text, replyTo)
        scheduler.schedule(chatId)
        return entry
    }

    /** Sends the entries of a room again, for example when its room screen opens. */
    suspend fun resume(chatId: ChatId) {
        if (outbox.observe(chatId).first().any { !it.isFailed }) scheduler.schedule(chatId)
    }

    /** The message with this id while it waits in the outbox. */
    suspend fun pending(entryId: ChatMessageId): ProductOutboxEntry? = outbox.get(entryId)

    /** Removes an entry. False when no entry has the id. */
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

    /** Sends the entries of a room, the oldest first. False when one has to go again later. */
    suspend fun processRoom(chatId: ChatId): Boolean {
        for (entry in outbox.observe(chatId).first().filter { !it.isFailed }) {
            if (process(entry) == Outcome.Later) return false
        }
        return true
    }

    private suspend fun process(entry: ProductOutboxEntry): Outcome = try {
        send(entry)
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        Timber.w(error, "truapi.chat.stage %s goes again later", entry.id)
        Outcome.Later
    }

    private suspend fun send(entry: ProductOutboxEntry): Outcome {
        val policy = rooms.policy(entry.chatId) ?: return Outcome.Later
        if (policy.messageStore == ChatRoomPolicy.MessageStore.HOST) {
            rooms.saveUserMessage(entry.chatId, entry.content(), entry.replyTo)
            outbox.remove(entry.id)
            return Outcome.Sent
        }

        val querying = rooms.querying(entry.chatId) ?: return Outcome.Later
        val action = ChatUserActionModel.NewMessage(
            entry.content(),
            entry.replyTo,
            messageId = entry.id,
            timestamp = entry.createdAt,
        )
        val result = querying.userAction(entry.chatId, action)
        val error = result.exceptionOrNull()
        if (error != null) return deliveryFailed(entry, error)

        // An open room shows the message that the product made, with what the user did to it in the meantime.
        (result.getOrNull() as? ChatUserActionOutcome.MessageCreated)?.let { created ->
            val current = outbox.get(entry.id) ?: entry
            feedListeners.deliver(entry.chatId, ProductFeedUpdateModel.New(created.message.withPendingActions(current)))
        }

        return sendPendingActions(entry.id, querying)
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
            // The product does not take the message, so it cannot go.
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

            // The worker does not run or did not answer in time, so the message goes again.
            else -> Outcome.Later
        }
    }

    private suspend fun fail(entry: ProductOutboxEntry, failure: ProductAttachmentFailure) {
        outbox.update(entry.id) { it.copy(failure = failure) }
    }

    private companion object {
        const val MAX_REFUSALS = 5
    }
}

/** The message of an entry. */
fun ProductOutboxEntry.content(): ChatMessage.Content = ChatMessage.Content.Text(text.orEmpty())

/** The message as the user sees it while it waits: with the edit and the reaction that wait for it. */
internal fun ProductRoomMessage.withPendingActions(entry: ProductOutboxEntry): ProductRoomMessage {
    val edited = entry.editedText?.let { text -> copy(content = content.withEditedText(text), editedAt = entry.editedAt) } ?: this
    return entry.reaction?.let { edited.adjustReaction(it, ProductParticipant.USER, placed = true) } ?: edited
}
