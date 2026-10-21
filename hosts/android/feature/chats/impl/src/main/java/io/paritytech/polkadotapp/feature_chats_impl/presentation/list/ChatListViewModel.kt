package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateService
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderersById
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.asAnyRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.domain.model.previewText
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
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallListPreviews
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.CallResolutionContext
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.buildCallResolutionContext
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.resolveCallState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.toCallPurposeUi
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatMessageMappingHelper
import io.paritytech.polkadotapp.feature_chats_impl.utils.toAttachmentType
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
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
    private val roomPolicyMapper: ChatListRoomPolicyMapper,
    private val userStateService: ChatRoomUserStateService,
    private val callPreviews: ProductRoomCallListPreviews,
) : BaseViewModel() {
    private val showsArchived = MutableStateFlow(false)

    /** The room whose mute duration the user picks now, or null. */
    val mutePicker = MutableStateFlow<ChatId?>(null)

    val state = combine(
        combine(
            interactor.subscribeChats().withMuteClock(System::currentTimeMillis),
            interactor.subscribeDrafts(),
            showsArchived,
            callPreviews.observe(),
            ::ListInputs,
        ),
        interactor.subscribeAllCustomMessageRenderers(),
        interactor.subscribePendingIncomingRequestsCount(),
        interactor.subscribeCallSignaling(),
        interactor.subscribeActiveCall()
    ) { (chatsAtTime, drafts, archivedAsked, ringing), customMessageRenderers, pendingRequestsCount, callSignaling, activeCall ->
        val (chatList, now) = chatsAtTime
        val sections = chatList.toSections()
        // The archived section closes by itself when the user unarchives its last room.
        val showsArchivedSection = archivedAsked && sections.archived.isNotEmpty()
        val shown = if (showsArchivedSection) sections.archived else sections.main
        val callContext = buildCallResolutionContext(callSignaling, activeCall)
        val chats = shown.map { it.toUi(drafts, customMessageRenderers, callContext, now, ringing[it.id]) }

        ChatListUiState(
            chats = chats.toImmutableList(),
            pendingRequestsCount = pendingRequestsCount,
            archivedCount = sections.archived.size,
            showsArchived = showsArchivedSection,
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

    fun onArchivedRowClick() {
        showsArchived.value = true
    }

    /** Back from the archived section to the main section. */
    fun onArchivedBack() {
        showsArchived.value = false
    }

    /** A muted room unmutes at once. Another room asks the user how long to mute it. */
    fun onMuteToggle(chat: ChatListUiState.ChatItem) {
        val userState = currentUserState(chat) ?: return
        if (userState.isMuted) {
            changeUserState { userStateService.unmute(chat.chatId) }
        } else {
            mutePicker.value = chat.chatId
        }
    }

    fun onMuteDurationSelected(duration: ChatMuteDuration) {
        val chatId = mutePicker.value ?: return
        mutePicker.value = null
        changeUserState { userStateService.mute(chatId, duration) }
    }

    fun onMutePickerDismiss() {
        mutePicker.value = null
    }

    fun onArchiveToggle(chat: ChatListUiState.ChatItem) {
        val userState = currentUserState(chat) ?: return
        changeUserState { userStateService.setArchived(chat.chatId, archived = !userState.isArchived) }
    }

    /** The state of the row as the list shows it now: a gesture can name a row that the list drew before a change. */
    private fun currentUserState(chat: ChatListUiState.ChatItem): ChatListUiState.UserState? {
        val shown = (state.value as? LoadingState.Loaded)?.data?.chats?.find { it.chatId == chat.chatId }
        return shown?.userState ?: chat.userState
    }

    private fun changeUserState(change: suspend () -> Result<Unit>) = launchUnit {
        change().onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }

    private suspend fun Chat.toUi(
        drafts: Map<ChatId, ChatDraft>,
        customMessageRenderers: CustomChatMessageRenderersById,
        callContext: CallResolutionContext,
        nowMs: Long,
        ringingText: String?,
    ): ChatListUiState.ChatItem {
        val draftText = drafts[id]?.text?.takeIf { it.isNotBlank() }
        val productPreview = roomPolicyMapper.productPreview(this)
        val previewModel = when {
            draftText != null -> DraftPreviewUiModel(timestamp = timestamp, text = draftText)
            // A ringing product room shows "Incoming call" until the call stops to ring (07-LATER §11.2).
            ringingText != null -> LastMessageUiModel.Text(timestamp = timestamp, isIncoming = true, message = ringingText)
            productPreview != null -> productPreview
            else -> preview.toUi(
                timestamp = timestamp,
                customMessageRenderers = customMessageRenderers,
                customPreviewDelegate = customPreviewRenderer,
                callContext = callContext,
            )
        }

        val userState = userStateAt(nowMs)

        return ChatListUiState.ChatItem(
            chatId = id,
            display = ChatDisplayUiModel(username = display.name, avatarModel = roomPolicyMapper.avatar(this)),
            badge = roomPolicyMapper.badge(unreadBadge),
            preview = previewModel,
            isMuted = userState?.isMuted == true,
            hasReaction = hasUnseenReaction,
            isOngoing = draftText == null && (ringingText != null || roomPolicyMapper.isOngoing(this)),
            userState = userState,
        )
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

            is ChatMessage.Content.ProductBody -> Text(
                timestamp = timestamp,
                isIncoming = isIncoming,
                message = content.body.previewText().orEmpty(),
            )

            is ChatMessage.Content.DataChannelAnswer,
            is ChatMessage.Content.DataChannelIceCandidate,
            is ChatMessage.Content.DataChannelClosed,
            is ChatMessage.Content.DeviceAdded,
            is ChatMessage.Content.DeviceRemoved,
            is ChatMessage.Content.CompactionCommit,
            is ChatMessage.Content.Deleted -> {
                Unsupported(
                    timestamp = timestamp,
                    isIncoming = isIncoming
                )
            }
        }
    }
}

/** What the list combines before it draws its rows. [ringing] has the preview text of each room that rings. */
private data class ListInputs(
    val chatsAtTime: Pair<List<Chat>, Long>,
    val drafts: Map<ChatId, ChatDraft>,
    val archivedAsked: Boolean,
    val ringing: Map<ChatId, String>,
)
