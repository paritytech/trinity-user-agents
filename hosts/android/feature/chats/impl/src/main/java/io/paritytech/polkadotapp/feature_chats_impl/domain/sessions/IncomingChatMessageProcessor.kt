package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onAttachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onContactAdded
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onDataChannelOffer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onEdited
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onLeftChat
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onReacted
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onReactionRemoved
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onTokenContent
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction.CompactionExpansionStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.download.FileDownloadStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toChatMessageOrUnsupported
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileDownloadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileDownload
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import timber.log.Timber
import javax.inject.Inject

class IncomingChatMessageProcessor @Inject constructor(
    private val chatEngine: ChatEngine,
    private val processedChatMessageRepository: ProcessedChatMessageRepository,
    private val contactsRepository: ContactsRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val callController: CallController,
    private val fileDownloadRepository: FileDownloadRepository,
    private val fileDownloadStarter: FileDownloadStarter,
    private val compactionExpansionStarter: CompactionExpansionStarter,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator
) {
    suspend fun processRaw(
        contactAccountId: AccountId,
        rawMessages: List<EncodedMessage>,
        placement: ChatMessagePlacement,
    ) {
        val contact = contactsRepository.getContact(contactAccountId)
        if (contact == null) {
            Timber.w("Dropping ${rawMessages.size} incoming messages: no contact for $contactAccountId")
            return
        }

        val parsedMessages = rawMessages.mapNotNull { encodedMessage ->
            encodedMessage.toChatMessageOrUnsupported(
                authorAccountId = contactAccountId,
                contactAccountId = contactAccountId,
                messageStatus = ChatMessage.Status.NEW
            )
                .logFailure("Dropping an undecodable incoming message of ${encodedMessage.size} bytes")
                .getOrNull()
        }

        val messageIds = parsedMessages.map { it.id }

        chatEngine.saveMessages(parsedMessages, ChatMessageSaveConflictStrategy.IGNORE, placement)

        if (parsedMessages.any { it.content is ChatMessage.Content.CompactionCommit }) {
            compactionExpansionStarter.startExpansion()
        }

        val claimedIds = processedChatMessageRepository.tryMarkProcessed(messageIds)

        parsedMessages.forEach { message ->
            if (message.id in claimedIds) {
                Timber.i("Processing ${message.id} ${message.content::class.simpleName}")
                dispatch(contact, message)
            } else {
                Timber.i("Skipping ${message.id} (${message.content::class.simpleName}) - already processed")
            }
        }
    }

    suspend fun processParsed(contactAccountId: AccountId, message: ChatMessage) {
        val contact = contactsRepository.getContact(contactAccountId)
        if (contact == null) {
            Timber.w("Dropping parsed message ${message.id}: no contact for $contactAccountId")
            return
        }

        if (message.content is ChatMessage.Content.CompactionCommit) {
            compactionExpansionStarter.startExpansion()
        }

        dispatch(contact, message)
    }

    private suspend fun dispatch(contact: Contact, chatMessage: ChatMessage) = runCatching {
        val contactAccountId = contact.accountId
        val chatId = ChatId.fromContact(contactAccountId)

        chatMessage.onTokenContent { content ->
            Timber.d("dispatch: received Token message from ${contact.username}, isVoIP=${content.isVoIP}, os=${content.operatingSystem}")
            if (content.isVoIP) {
                contactsRepository.updateContactVoipPushToken(contactAccountId, content.token)
            } else {
                contactsRepository.updateContactPushToken(contactAccountId, content.token, content.operatingSystem)
            }
        }.onReacted {
            val reaction = createChatMessageReaction(contactAccountId, it.messageId, it.content, chatMessage.timestamp)
            chatMessageRepository.addReaction(reaction, chatId)
        }.onReactionRemoved {
            val reaction = createChatMessageReaction(contactAccountId, it.messageId, it.content, chatMessage.timestamp)
            chatMessageRepository.removeMessageReaction(reaction, chatId)
        }.onLeftChat {
            // Ignore a stale LEFT_CHAT from a prior session (reused topic) that predates this re-add.
            if (chatMessage.timestamp >= contact.addedAt.toEpochMilliseconds()) {
                contactsRepository.setPeerLeft(contactAccountId, true)
            }
        }.onContactAdded {
            contactsRepository.setPeerLeft(contactAccountId, false)
        }.onEdited { editedContent ->
            chatEngine.saveRevision(
                messageId = editedContent.messageId,
                content = editedContent.content,
                chatId = chatId,
                timestamp = chatMessage.timestamp
            )
        }.onDataChannelOffer { offer ->
            callController.initiateIncomingCall(
                chatId = chatId,
                offerId = chatMessage.id,
                callerName = contact.username ?: fallbackUsernameGenerator.generateFromAccountId(contactAccountId),
                withVideo = offer.purpose == ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL
            )
        }.onAttachment { attachment ->
            fileDownloadRepository.addDownloadToQueue(
                FileDownload.new(
                    messageId = chatMessage.id,
                    chatId = chatId,
                    identifier = attachment.identifier,
                    ticket = attachment.ticket,
                    nodeUrl = attachment.nodeUrl,
                    mimeType = attachment.meta.mimeType
                )
            )
            fileDownloadStarter.startDownloading()
        }
    }.logFailure(
        "Failed to process message ${chatMessage.id} " +
            "(content=${chatMessage.content::class.simpleName}, status=${chatMessage.status})"
    )

    private fun createChatMessageReaction(
        contactAccountId: AccountId,
        messageId: ChatMessageId,
        content: ChatMessageReactionContent,
        timestamp: Timestamp
    ): ChatMessageReaction {
        return ChatMessageReaction(
            messageId = messageId,
            content = content,
            origin = ChatMessageOrigin.Contact(contactAccountId),
            timestamp = timestamp
        )
    }
}
