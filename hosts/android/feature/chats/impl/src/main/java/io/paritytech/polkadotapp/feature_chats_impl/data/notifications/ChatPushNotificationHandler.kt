package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.asAnyRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.ChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.MESSAGE_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.PUSH_ID_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.StrippedChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction.CompactionExpansionStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toChatMessageOrUnsupported
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import io.paritytech.polkadotapp.feature_chats_impl.domain.getCustomMessageRenderer
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatMessageMappingHelper
import io.paritytech.polkadotapp.feature_chats_impl.utils.emojiRes
import io.paritytech.polkadotapp.feature_chats_impl.utils.nameRes
import io.paritytech.polkadotapp.feature_chats_impl.utils.toAttachmentType
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementRequestDecoder
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationHandler
import timber.log.Timber
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

// legacy fields
private const val PUSH_ID = "pushId"
private const val MESSAGE = "message"

// v2 fields
private const val SENDER_PUBKEY = "sender_pubkey"
private const val STATEMENT_TOPIC = "statement_topic"
private const val STATEMENT_DATA = "statement_data"

internal class ChatPushNotificationHandler @Inject constructor(
    private val incomingChatPushDecoder: IncomingChatPushDecoder,
    private val chatMessageRepository: ChatMessageRepository,
    private val chatNotificationPublisher: ChatNotificationPublisher,
    private val tokenAmountFormatter: TokenAmountFormatter,
    private val messageMappingHelper: ChatMessageMappingHelper,
    private val statementRequestDecoder: StatementRequestDecoder,
    private val chatActiveTracker: ChatActiveTracker,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val chatEngine: ChatEngine,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
    private val callController: CallController,
    private val processedChatMessageRepository: ProcessedChatMessageRepository,
    private val contactsRepository: ContactsRepository,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val compactionExpansionStarter: CompactionExpansionStarter,
    @param:ApplicationContext private val appContext: Context
) : PushNotificationHandler {
    override suspend fun canHandle(data: Map<String, String>): Boolean {
        return data.isLegacyChatPush() || data.isNewSpecChatPush()
    }

    override suspend fun handle(data: Map<String, String>) {
        if (data.isNewSpecChatPush()) {
            handleNewSpec(data)
        } else {
            handleLegacy(data)
        }
    }

    private suspend fun handleLegacy(data: Map<String, String>) {
        val decoded = incomingChatPushDecoder.decode(data)
            .onFailure { Timber.e(it, "Failed to decode chat push") }
            .getOrNull() ?: return

        val pushContent = decoded.content
        if (pushContent is ChatPushContent.Full) {
            chatEngine.saveMessage(pushContent.message, ChatMessageSaveConflictStrategy.IGNORE)
        }

        val callerName = decoded.contact.username
            ?: fallbackUsernameGenerator.generateFromAccountId(decoded.contact.accountId)

        val offer = pushContent.dataChannelOfferOrNull()
        if (offer != null) {
            handleIncomingCallOffer(decoded.chatId, pushContent.messageId, offer, callerName)
        } else {
            publishIncomingMessageNotification(decoded.chatId, callerName) { pushContent.toMessageText(decoded.chatId) }
        }
    }

    private suspend fun handleIncomingCallOffer(
        chatId: ChatId,
        offerId: String,
        offer: ChatMessage.Content.DataChannelOffer,
        callerName: String,
    ) {
        if (!processedChatMessageRepository.tryMarkProcessed(offerId)) return

        callController.initiateIncomingCall(
            chatId = chatId,
            offerId = offerId,
            callerName = callerName,
            withVideo = offer.purpose == ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL,
        )
    }

    private suspend fun handleNewSpec(data: Map<String, String>) {
        val senderAccountId = data.getValue(SENDER_PUBKEY).fromHex().toDataByteArray()

        val contact = contactsRepository.getContact(senderAccountId) ?: return
        if (contact.isBlocked) return

        val encryptedStatementData = data[STATEMENT_DATA]?.fromHex() ?: return // TODO: handle statement fetch

        val ownMetaAccount = accountRepository.getAccountById(contact.ourMetaAccountId) ?: return
        val ourStatementAccountId = ownMetaAccount.accountIdIn(chainRegistry.getChain(knownChains.people))

        val encodedMessages = statementRequestDecoder
            .decodeMessages(
                sharedSecretDerivationDomain = contact.sharedSecretDerivationDomain,
                peerEncryptionPublicKey = contact.chatKey,
                ourStatementAccountId = ourStatementAccountId,
                encryptedRequest = encryptedStatementData,
            )
            .logFailure("Failed to decode incoming push statement data")
            .getOrNull()
            .orEmpty()

        if (encodedMessages.isEmpty()) return

        val chatMessages = encodedMessages.mapNotNull { encoded ->
            encoded.toChatMessageOrUnsupported(
                authorAccountId = contact.accountId,
                contactAccountId = contact.accountId,
                messageStatus = ChatMessage.Status.NEW
            ).getOrNull()
        }

        if (chatMessages.isEmpty()) return

        val existingStatuses = chatMessageRepository.getMessageStatuses(chatMessages.map { it.id })
        val previouslyUnseen = chatMessages.filterNot { it.id in existingStatuses }

        for (chatMessage in chatMessages.sortedBy { it.timestamp }) {
            chatEngine.saveMessage(chatMessage, ChatMessageSaveConflictStrategy.IGNORE)
        }

        if (chatMessages.any { it.content is ChatMessage.Content.CompactionCommit }) {
            compactionExpansionStarter.startExpansion()
        }

        if (previouslyUnseen.isEmpty()) return

        val contactChatId = ChatId.fromContact(contact.accountId)
        val displayName = contact.username
            ?: fallbackUsernameGenerator.generateFromAccountId(contact.accountId)

        for (chatMessage in previouslyUnseen.sortedBy { it.timestamp }) {
            publishIncomingMessageNotification(contactChatId, displayName) { chatMessage.content.toMessageText(contactChatId) }
        }
    }

    private suspend fun publishIncomingMessageNotification(
        contactChatId: ChatId,
        displayName: String,
        formatMessageText: suspend () -> String?,
    ) {
        val appState = appLifecycleObserver.getCurrentState()
        val activeChatId = chatActiveTracker.getActive()

        if (activeChatId == contactChatId && appState == AppLifecycleState.FOREGROUND) return

        val messageText = formatMessageText()

        if (messageText != null) {
            chatNotificationPublisher
                .publishNewMessageReceived(
                    chatId = contactChatId,
                    username = displayName,
                    text = messageText
                )
        }
    }

    private fun ChatPushContent.dataChannelOfferOrNull(): ChatMessage.Content.DataChannelOffer? {
        val content = when (this) {
            is ChatPushContent.Full -> message.content
            is ChatPushContent.Stripped -> (content as? StrippedChatPushContent.Regular)?.content
        }

        return content as? ChatMessage.Content.DataChannelOffer
    }

    private suspend fun ChatPushContent.toMessageText(chatId: ChatId): String? {
        return when (this) {
            is ChatPushContent.Full -> message.content.toMessageText(chatId)

            is ChatPushContent.Stripped -> when (val content = content) {
                is StrippedChatPushContent.Regular -> content.content.toMessageText(chatId)
                is StrippedChatPushContent.CoinagePayment -> paymentText(content.totalValue)
            }
        }
    }

    private suspend fun paymentText(totalValue: Balance): String {
        val tokenAmount = messageMappingHelper.extractTokenAmount(totalValue)

        val value = tokenAmountFormatter.formatTokenAmount(
            tokenAmount = tokenAmount,
            precision = RoundPrecision.DEFAULT
        )

        return appContext.getString(
            RCommon.string.chat_notification_payment,
            value
        )
    }

    private suspend fun ChatMessage.Content.toMessageText(chatId: ChatId): String? {
        return when (val content = this) {
            is ChatMessage.Content.Text -> content.text

            is ChatMessage.Content.CoinagePayment -> paymentText(content.totalValue)

            is ChatMessage.Content.ContactAdded -> {
                appContext.getString(RCommon.string.chat_notification_contact_added)
            }

            is ChatMessage.Content.LeftChat -> null

            is ChatMessage.Content.Token -> null

            is ChatMessage.Content.RichText -> {
                if (content.attachments.isNotEmpty()) {
                    val attachmentType = content.attachments.first().meta.toAttachmentType()
                    val emoji = appContext.getString(attachmentType.emojiRes())
                    val text = content.text
                    if (text != null) "$emoji $text" else "$emoji ${appContext.getString(attachmentType.nameRes())}"
                } else {
                    content.text
                }
            }

            is ChatMessage.Content.Reacted -> {
                appContext.getString(RCommon.string.chat_notification_reaction_added, content.content.emoji)
            }

            is ChatMessage.Content.ReactionRemoved -> null

            is ChatMessage.Content.Unsupported -> unsupportedText()

            is ChatMessage.Content.Custom<*> -> {
                val renderer = chatEngine.getCustomMessageRenderer(chatId, content.rendererId)
                    ?: return unsupportedText()

                @Suppress("UNCHECKED_CAST")
                renderer.asAnyRenderer()
                    .formatNotificationContent(content as ChatMessage.Content.Custom<Any?>)
                    .logFailure("Failed to format custom message content")
                    .getOrElse { unsupportedText() }
            }

            is ChatMessage.Content.DataChannelOffer -> {
                appContext.getString(RCommon.string.chat_last_message_call_started)
            }

            is ChatMessage.Content.Edited,
            is ChatMessage.Content.DataChannelAnswer,
            is ChatMessage.Content.DataChannelIceCandidate,
            is ChatMessage.Content.DataChannelClosed,
            is ChatMessage.Content.DeviceAdded,
            is ChatMessage.Content.DeviceRemoved,
            is ChatMessage.Content.CompactionCommit,
            is ChatMessage.Content.CompactionUnavailable -> null

            is ChatMessage.Content.ChatAccepted,
            is ChatMessage.Content.DeviceChatAccepted -> {
                appContext.getString(RCommon.string.chat_notification_request_approved)
            }

            is ChatMessage.Content.ChatRequest -> content.welcome?.text
        }
    }

    private fun unsupportedText() = appContext.getString(RCommon.string.chat_message_unsupported)

    private fun Map<String, String>.isLegacyChatPush(): Boolean {
        return this[PUSH_ID_KEY] != null && this[MESSAGE_KEY] != null
    }

    private fun Map<String, String>.isNewSpecChatPush(): Boolean {
        return this[SENDER_PUBKEY] != null && this[STATEMENT_TOPIC] != null
    }
}

internal fun ChatMessage.Content.isDisplayableAsPush(): Boolean = when (this) {
    is ChatMessage.Content.Text,
    is ChatMessage.Content.RichText,
    is ChatMessage.Content.Reacted,
    is ChatMessage.Content.ContactAdded,
    is ChatMessage.Content.ChatAccepted,
    is ChatMessage.Content.DeviceChatAccepted,
    is ChatMessage.Content.CoinagePayment,
    is ChatMessage.Content.DataChannelOffer,
    is ChatMessage.Content.ChatRequest,
    is ChatMessage.Content.Unsupported,
    is ChatMessage.Content.Custom<*> -> true

    is ChatMessage.Content.LeftChat,
    is ChatMessage.Content.Token,
    is ChatMessage.Content.ReactionRemoved,
    is ChatMessage.Content.Edited,
    is ChatMessage.Content.DataChannelAnswer,
    is ChatMessage.Content.DataChannelIceCandidate,
    is ChatMessage.Content.DataChannelClosed,
    is ChatMessage.Content.DeviceAdded,
    is ChatMessage.Content.DeviceRemoved,
    is ChatMessage.Content.CompactionCommit,
    is ChatMessage.Content.CompactionUnavailable -> false
}
