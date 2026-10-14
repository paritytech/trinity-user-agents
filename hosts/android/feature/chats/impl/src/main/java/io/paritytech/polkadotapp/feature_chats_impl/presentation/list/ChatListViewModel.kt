package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.combineToPair
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderersById
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.asAnyRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.DraftPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Attachments
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Call
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.ChatAccepted
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.ContactAdded
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Custom
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.LeftChat
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Payment
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Reacted
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.RemovedReaction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Text
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Token
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel.Unsupported
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.CallResolutionContext
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.buildCallResolutionContext
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.resolveCallState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.toCallPurposeUi
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatMessageMappingHelper
import io.paritytech.polkadotapp.feature_chats_impl.utils.toAttachmentType
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@Suppress("USELESS_CAST")
@HiltViewModel
class ChatListViewModel @Inject constructor(
    private val interactor: ChatListInteractor,
    private val router: ChatsRouter,
    private val messageMappingHelper: ChatMessageMappingHelper,
) : BaseViewModel() {
    val state = combine(
        combineToPair(
            interactor.subscribeChats(),
            interactor.subscribeDrafts()
        ),
        interactor.subscribeAllCustomMessageRenderers(),
        interactor.subscribePendingIncomingRequestsCount(),
        interactor.subscribeCallSignaling(),
        interactor.subscribeActiveCall()
    ) { (chatList, drafts), customMessageRenderers, pendingRequestsCount, callSignaling, activeCall ->
        val callContext = buildCallResolutionContext(callSignaling, activeCall)
        val chats = chatList.map { it.toUi(drafts, customMessageRenderers, callContext) }

        ChatListUiState(
            chats = chats.toImmutableList(),
            pendingRequestsCount = pendingRequestsCount
        )
    }
        .withLoading()
        .inBackground()
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = LoadingState.Loading
        )

    fun onSearchBarClick() {
        router.openChatSearch()
    }

    fun onChatClick(chat: ChatListUiState.ChatItem) = launchUnit {
        if (!interactor.handleChatOpen(chat.chatId)) {
            router.openChatFeed(ChatFeedPayload.existingChat(chat.chatId))
        }
    }

    fun onAddContactClick() {
        router.openAddContact()
    }

    fun onNewRequestsClick() {
        router.openMessageRequests()
    }

    private suspend fun Chat.toUi(
        drafts: Map<ChatId, ChatDraft>,
        customMessageRenderers: CustomChatMessageRenderersById,
        callContext: CallResolutionContext,
    ): ChatListUiState.ChatItem {
        val draftText = drafts[id]?.text?.takeIf { it.isNotBlank() }
        val previewModel = if (draftText != null) {
            DraftPreviewUiModel(timestamp = timestamp, text = draftText)
        } else {
            preview.toUi(
                timestamp = timestamp,
                customMessageRenderers = customMessageRenderers,
                customPreviewDelegate = customPreviewRenderer,
                callContext = callContext,
            )
        }

        return ChatListUiState.ChatItem(
            chatId = id,
            display = display.toUi(),
            badge = unreadBadge.toUi(),
            preview = previewModel,
            // TODO: wire mute state from domain Chat once the field exists.
            isMuted = false,
            hasReaction = hasUnseenReaction,
        )
    }

    private fun ChatSummaryBadge.toUi(): ChatListUiState.Badge {
        return when (this) {
            is ChatSummaryBadge.Notification,
            is ChatSummaryBadge.None -> ChatListUiState.Badge.None
            is ChatSummaryBadge.Unread -> ChatListUiState.Badge.Unread(count)
        }
    }

    private suspend fun ChatPreview.toUi(
        timestamp: Timestamp,
        customMessageRenderers: CustomChatMessageRenderersById,
        customPreviewDelegate: CustomChatPreviewRenderer<*>?,
        callContext: CallResolutionContext,
    ): ChatPreviewUiModel? {
        return when (this) {
            is ChatPreview.Message -> message.toUi(customMessageRenderers, callContext)
            is ChatPreview.Custom<*> -> toUi(timestamp, customPreviewDelegate)
            is ChatPreview.EmptyChat -> null
        }
    }

    private fun ChatPreview.Custom<*>.toUi(timestamp: Timestamp, customPreviewDelegate: CustomChatPreviewRenderer<*>?): ChatPreviewUiModel {
        // ChatEngine converts CustomPreviewData.FromMessage into ChatPreview.Message before
        // this point, so here the data is always a RendererPayload we can unwrap for the renderer.
        val payload = (data as? CustomPreviewData.RendererPayload<*>) ?: return Unsupported(
            timestamp = timestamp,
            isIncoming = false,
        )
        val delegate = customPreviewDelegate ?: return Unsupported(
            timestamp = timestamp,
            isIncoming = false,
        )
        return CustomChatPreviewUiModel(
            data = payload.data,
            timestamp = timestamp,
            renderer = delegate.asAnyRenderer()
        )
    }

    private suspend fun ChatMessage.toUi(
        customRenderers: CustomChatMessageRenderersById,
        callContext: CallResolutionContext,
    ): ChatPreviewUiModel? {
        return when (val content = content) {
            is ChatMessage.Content.Text -> Text(
                timestamp = timestamp,
                message = content.text,
                isIncoming = isIncoming
            )

            is ChatMessage.Content.ContactAdded -> ContactAdded(
                timestamp = timestamp,
                isIncoming = isIncoming
            )

            is ChatMessage.Content.Token -> Token(
                timestamp = timestamp,
                isIncoming = isIncoming
            )

            is ChatMessage.Content.CoinagePayment -> {
                val tokenAmount = messageMappingHelper.extractTokenAmount(content)

                Payment(
                    timestamp = timestamp,
                    isIncoming = isIncoming,
                    tokenAmount = tokenAmount,
                    paymentStatus = messageMappingHelper.mapPaymentStatus(content.status)
                )
            }

            is ChatMessage.Content.RichText -> if (content.attachments.isNotEmpty()) {
                Attachments(
                    timestamp = timestamp,
                    isIncoming = isIncoming,
                    type = content.attachments.first().meta.toAttachmentType(),
                    count = content.attachments.size,
                    message = content.text,
                )
            } else {
                Text(
                    timestamp = timestamp,
                    isIncoming = isIncoming,
                    message = content.text.orEmpty(),
                )
            }

            is ChatMessage.Content.Reacted -> Reacted(
                timestamp = timestamp,
                isIncoming = isIncoming,
                emoji = content.content.emoji
            )

            is ChatMessage.Content.ReactionRemoved -> RemovedReaction(
                timestamp = timestamp,
                isIncoming = isIncoming,
            )

            is ChatMessage.Content.Unsupported -> Unsupported(
                timestamp = timestamp,
                isIncoming = isIncoming,
            )

            is ChatMessage.Content.LeftChat -> LeftChat(
                timestamp = timestamp,
                isIncoming = isIncoming
            )

            is ChatMessage.Content.Edited ->
                when {
                    content.content.attachments.isNotEmpty() -> Attachments(
                        timestamp = timestamp,
                        isIncoming = isIncoming,
                        type = content.content.attachments.first().meta.toAttachmentType(),
                        count = content.content.attachments.size,
                        message = content.content.text,
                    )

                    content.content.text != null -> Text(
                        timestamp = timestamp,
                        isIncoming = isIncoming,
                        message = content.content.text.orEmpty(),
                    )

                    else -> Unsupported(
                        timestamp = timestamp,
                        isIncoming = isIncoming,
                    )
                }

            is ChatMessage.Content.Custom<*> -> {
                val renderer = customRenderers[content.rendererId] ?: return Unsupported(
                    timestamp = timestamp,
                    isIncoming = isIncoming,
                )

                Custom(
                    timestamp = timestamp,
                    isIncoming = isIncoming,
                    renderer = renderer.asAnyRenderer(),
                    content = content.content
                )
            }

            is ChatMessage.Content.ChatAccepted,
            is ChatMessage.Content.DeviceChatAccepted -> ChatAccepted(
                timestamp = timestamp,
                isIncoming = isIncoming,
            )

            is ChatMessage.Content.ChatRequest -> Text(
                timestamp = timestamp,
                isIncoming = isIncoming,
                message = content.welcome?.text.orEmpty()
            )

            is ChatMessage.Content.DataChannelOffer -> Call(
                timestamp = timestamp,
                isIncoming = isIncoming,
                purpose = content.purpose.toCallPurposeUi(),
                state = resolveCallState(offer = this, callContext = callContext),
            )

            is ChatMessage.Content.CompactionUnavailable -> {
                LastMessageUiModel.CompactionUnavailable(
                    timestamp = timestamp,
                    isIncoming = isIncoming
                )
            }

            is ChatMessage.Content.DataChannelAnswer,
            is ChatMessage.Content.DataChannelIceCandidate,
            is ChatMessage.Content.DataChannelClosed,
            is ChatMessage.Content.DeviceAdded,
            is ChatMessage.Content.DeviceRemoved,
            is ChatMessage.Content.CompactionCommit -> {
                Unsupported(
                    timestamp = timestamp,
                    isIncoming = isIncoming
                )
            }
        }
    }
}
