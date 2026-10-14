package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import android.Manifest
import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.awaitLoaded
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.OneShotEventChannel
import io.paritytech.polkadotapp.common.utils.collectionIndexOrNull
import io.paritytech.polkadotapp.common.utils.combineToPair
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapResult
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.common.utils.throttleLatest
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressParcel
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.computeChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isUser
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.FirstNewMessageInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.HighlightedMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageRevisionUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.isUnread
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.toOpenChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.asAttachmentError
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.asChatRequestError
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.InitiateCallResult
import io.paritytech.polkadotapp.feature_chats_impl.presentation.error.BusyInAnotherChatPresentationError
import io.paritytech.polkadotapp.feature_chats_impl.presentation.error.toPresentationError
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatInputUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMessagesState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatRequestAnswerProgress
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatSendMessageInputState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.InputMessageRelation
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.MessagePopUpSelection
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.clearRelation
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.isNotNone
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.sendableMessageState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.sendableMessageText
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.AttachmentExecutor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.AttachmentResult
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.TransferMethodPayload
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.math.max
import kotlin.time.Duration.Companion.milliseconds

private const val MAX_CHAT_MESSAGE_LENGTH = 500
private const val NEWEST_MESSAGE_POSITION = 0

@HiltViewModel
class ChatFeedViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val router: ChatsRouter,
    private val interactor: ChatFeedInteractor,
    private val messageUiMapper: ChatMessageUiMapper,
    private val clipboardService: ClipboardService,
    private val menuActionsProvider: ChatMenuActionsProvider,
    private val permissionAsker: PermissionAsker,
    private val attachmentExecutor: AttachmentExecutor,
    private val dispatchers: CoroutineDispatchers,
    draftControllerFactory: ChatDraftController.Factory,
    callStateTracker: CallStateTracker
) : BaseViewModel(), ChatFeedContract {
    private val payload: ChatFeedPayload = savedStateHandle.getPayload()

    private val openChatRequest: OpenChatRequest = payload.toOpenChatRequest()
    private val chatId = openChatRequest.computeChatId()

    private val messagePopupSection = MutableStateFlow<MessagePopUpSelection?>(null)
    private val _highlightEvents = OneShotEventChannel<HighlightedMessage>()
    override val highlightEvents = _highlightEvents.receiveAsFlow()
    override val scrollToPosition = MutableSharedFlow<Int>()
    private val lastReadMessageTimestamp = MutableStateFlow<Timestamp>(0)

    private val originDisplayResolver = flowOf { interactor.getMessageOriginDisplayResolver(openChatRequest) }
        .shareInBackground()

    private val reactions = interactor.subscribeChatReactions(chatId).shareInBackground()

    private val revisions = interactor.subscribeRevisions(chatId).shareInBackground()

    private val customMessageRenderers = flowOf { interactor.getCustomMessageRenderersForChat(chatId) }

    private val animatesMessageReveal = flowOf { interactor.animatesMessageReveal(chatId) }
        .shareInBackground()

    private val revealedUpTo = MutableStateFlow<Timestamp?>(null)

    private val gatedDomainMessages = combine(
        interactor.subscribeMessages(chatId),
        animatesMessageReveal,
        revealedUpTo,
    ) { domainMessages, animates, revealed ->
        gateMessageReveal(domainMessages, animates, revealed)
    }.stateInBackground(initialValue = GatedMessages(visibleMessages = emptyList(), revealingMessageId = null, revealComplete = false))

    override val revealingMessageId = gatedDomainMessages
        .map { it.revealingMessageId }
        .distinctUntilChanged()
        .stateInBackground(initialValue = null)

    private val revealComplete = gatedDomainMessages
        .map { it.revealComplete }
        .distinctUntilChanged()
        .stateInBackground(initialValue = false)

    private val messages = combine(
        combineToPair(
            gatedDomainMessages.map { it.visibleMessages },
            callStateTracker.observeActiveCall().map { state -> state?.takeIf { it.chatId == chatId } }
        ),
        reactions,
        originDisplayResolver,
        revisions,
        customMessageRenderers
    ) { (messages, activeCall), reactions, originDisplayResolver, revisionsMap, customMessageRenderers ->
        messageUiMapper.map(messages, originDisplayResolver, reactions, revisionsMap, customMessageRenderers, activeCall)
    }.shareInBackground()

    override val chatDisplay = interactor.observeChatDisplay(openChatRequest)
        .mapResult { it.toUi() }
        .withLoading()
        .stateInBackground(initialValue = LoadingState.Loading)

    override val headerRenderer = flowOf {
        interactor.getHeaderRendererForChat(chatId)
    }.stateInBackground(initialValue = null)

    override val config = interactor.getChatConfig(openChatRequest)
        .stateInBackground(initialValue = ChatConfig.Default)

    private val chatInputStateType = interactor.subscribeUserInputState(openChatRequest)
        .distinctUntilChanged()
        .shareInBackground()

    override val menuRenderer = flowOf {
        interactor.getMenuRendererForChat(chatId)
    }.stateInBackground(initialValue = null)

    override val toolbarActions = combine(
        chatInputStateType,
        menuRenderer,
    ) { inputStateType, renderer ->
        val hasRenderer = renderer != null
        menuActionsProvider.getToolbarActions(openChatRequest, inputStateType, addMenuIconForcibly = hasRenderer)
    }.stateInBackground(initialValue = persistentListOf())

    override val popupState = combine(
        messagePopupSection,
        reactions,
        originDisplayResolver,
        messages,
        chatInputStateType,
    ) { popupSelection, reactions, originDisplayResolver, messagesList, chatInputStateType ->
        if (popupSelection == null) return@combine null

        messageUiMapper.constructPopUpState(popupSelection, reactions, originDisplayResolver, messagesList, chatInputStateType)
    }.stateIn(this, SharingStarted.Eagerly, null)

    private val messageInputState = MutableStateFlow(ChatSendMessageInputState())
    private val chatRequestAnswerProgress = MutableStateFlow(ChatRequestAnswerProgress.None)

    private val draftController = draftControllerFactory.create(this, openChatRequest, messageInputState)

    override val chatInputUiState = combine(
        messageInputState,
        chatInputStateType,
        chatRequestAnswerProgress
    ) { messageInput, chatInputStateType, answerProgress ->
        chatInputStateType.toChatInputUiState(messageInput, answerProgress)
    }.stateIn(this, SharingStarted.Eagerly, ChatInputUiState.Hidden)

    private val firstNewMessageInfo = messages
        .take(1)
        .map { messagesList ->
            val index = messagesList.indexOfLast { it.isUnread() }.collectionIndexOrNull()

            if (index != null) {
                FirstNewMessageInfo(
                    index = index,
                    messageId = messagesList[index].id
                )
            } else null
        }
        .shareInBackground()

    private val unreadCounter = combine(
        messages,
        lastReadMessageTimestamp.throttleLatest(50.milliseconds),
    ) { messagesList, lastReadTimestamp ->
        messagesList.count { it.isUnread() && it.timestamp > lastReadTimestamp }
    }.shareInBackground()

    override val chatMessagesState = combine(
        messages,
        firstNewMessageInfo,
        unreadCounter.distinctUntilChanged(),
    ) { messagesList, firstNewMessageInfo, unreadCounter ->
        ChatMessagesState(
            messages = messagesList,
            unreadCounter = unreadCounter,
            firstNewMessageInfo = firstNewMessageInfo
        )
    }
        .withLoading()
        .stateIn(CoroutineScope(coroutineContext + dispatchers.computation), SharingStarted.Eagerly, LoadingState.Loading)

    override val menuState = MutableStateFlow(ChatMenuState())

    override val footerRenderer = combine(
        flowOf { interactor.getFooterRendererForChat(chatId) },
        revealComplete,
    ) { renderer, revealed ->
        renderer.takeIf { revealed }
    }.stateInBackground(initialValue = null)

    override val customAppearance = flowOf {
        interactor.getCustomAppearanceForChat(chatId)
    }.stateInBackground(initialValue = null)

    init {
        seedMessageRevealProgress()
        handleMarkMessagesAsReadTrigger()
        markNonDisplayableMessagesAsRead()
        draftController.start()
        jumpToTargetMessageIfNeeded()
    }

    override fun onCleared() {
        interactor.setChatInactive()
        draftController.flush()
        super.onCleared()
    }

    override fun onMessageChange(message: String) {
        messageInputState.updateInput(message)
    }

    override fun onBackClick() {
        router.back()
    }

    override fun onMessageRevealComplete(messageId: ChatMessageId) = launchUnit {
        val message = gatedDomainMessages.value.visibleMessages.find { it.id == messageId } ?: return@launchUnit
        revealedUpTo.value = message.timestamp
        interactor.setLastRevealedTimestamp(chatId, message.timestamp)

        val allRevealed = interactor.subscribeMessages(chatId).first().none { it.timestamp > message.timestamp && it.animatesReveal }
        if (allRevealed) scrollToPosition.emit(NEWEST_MESSAGE_POSITION)
    }

    private fun seedMessageRevealProgress() = launchUnit {
        revealedUpTo.value = interactor.getLastRevealedTimestamp(chatId) ?: 0L
    }

    private fun gateMessageReveal(
        domainMessages: List<ChatMessage>,
        animates: Boolean,
        revealedUpTo: Timestamp?,
    ): GatedMessages {
        if (!animates) return GatedMessages(visibleMessages = domainMessages, revealingMessageId = null, revealComplete = true)
        if (revealedUpTo == null) return GatedMessages(visibleMessages = emptyList(), revealingMessageId = null, revealComplete = false)

        val revealing = domainMessages
            .filter { it.timestamp > revealedUpTo && it.animatesReveal }
            .minByOrNull { it.timestamp }
        val visible = if (revealing == null) {
            domainMessages
        } else {
            domainMessages.filter { it.timestamp <= revealing.timestamp }
        }
        return GatedMessages(visibleMessages = visible, revealingMessageId = revealing?.id, revealComplete = revealing == null)
    }

    override fun onPayClick() = launchUnit {
        val inputState = (chatInputStateType.first() as? ChatUserInputState.SendMessage) ?: return@launchUnit
        val paymentAddress = when (val paymentState = inputState.paymentState) {
            is ChatUserInputState.SendMessage.Payment.Available -> paymentState.paymentAddress
            else -> return@launchUnit
        }
        val username = chatDisplay.awaitLoaded().username

        val extractedAddressParcel = ExtractedAddressParcel(
            display = username,
            type = ExtractedAddress.DisplayType.USERNAME,
            accountId = paymentAddress.value
        )
        router.openEnterAmount(
            SendEnterAmountPayload(
                showTransactionResult = false,
                transferMethod = TransferMethodPayload.CoinsViaChat(extractedAddressParcel),
                amountPreset = null,
            )
        )
    }

    override fun onSendMessageClick() {
        val sendableMessageState = chatInputUiState.value.sendableMessageState() ?: return
        val text = sendableMessageState.sendableMessageText() ?: return

        launch {
            val inputType = chatInputStateType.first()
            val inputRelation = sendableMessageState.relation

            messageInputState.clear()

            when (inputType) {
                ChatUserInputState.SendChatRequest -> {
                    if (inputRelation.isNotNone()) {
                        Timber.w("Unexpected input relation for chat request: $inputRelation. It will be ignored")
                    }

                    sendChatRequest(text)
                }

                is ChatUserInputState.SendMessage -> sendMessageToActiveChat(text, inputRelation)

                ChatUserInputState.AcceptChatRequest,
                ChatUserInputState.UnblockUser,
                ChatUserInputState.Nothing,
                ChatUserInputState.PeerLeft,
                ChatUserInputState.UserDeclinedChatRequest,
                ChatUserInputState.WaitChatRequestApproval -> {
                    Timber.w("Unexpected chat input state received: $inputType")
                }
            }
        }
    }

    override fun setChatActive() {
        interactor.setChatActive(chatId)
    }

    override fun setChatInactive() {
        interactor.setChatInactive()
    }

    override fun onClearReply() {
        messageInputState.update { it.copy(relation = InputMessageRelation.None) }
    }

    override fun onCloseMenu() {
        menuState.update { it.copy(isVisible = false) }
    }

    override fun onLeaveChatRequest() = launchUnit {
        val username = chatDisplay.awaitLoaded().username
        menuState.update { it.copy(type = ChatMenuType.LeaveConfirmation(username)) }
    }

    override fun onLeaveChatConfirm() = launchUnit {
        interactor.leaveChat(chatId)
            .onSuccess { router.back() }
            .onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }

    override fun onBlockUserRequest() = launchUnit {
        val username = chatDisplay.awaitLoaded().username
        menuState.update { it.copy(type = ChatMenuType.BlockConfirmation(username)) }
    }

    override fun onBlockUserConfirm() = launchUnit {
        interactor.blockUser(chatId)
            .onSuccess { router.back() }
            .onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }

    override fun onUnblockUserClick() = launchUnit {
        interactor.unblockUser(chatId)
            .onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }

    override fun onStartCallClick(withVideo: Boolean) = launchUnit {
        val permissions = buildList {
            add(Manifest.permission.RECORD_AUDIO)
            if (withVideo) add(Manifest.permission.CAMERA)
        }
        val result = permissionAsker.askPermission(*permissions.toTypedArray())

        if (result == PermissionResult.GRANTED) {
            val username = chatDisplay.awaitLoaded().username
            when (interactor.initiateCall(chatId, username, withVideo)) {
                InitiateCallResult.DONE -> Unit
                InitiateCallResult.BUSY_IN_ANOTHER_CHAT ->
                    showPresentationError(BusyInAnotherChatPresentationError())
            }
        }

        onCloseMenu()
    }

    override fun onMenuClick() = launchUnit {
        val customRenderer = menuRenderer.value
        val type = if (customRenderer != null) {
            ChatMenuType.Custom(customRenderer)
        } else {
            val chatInputState = chatInputStateType.first()
            val actions = menuActionsProvider.getChatMenuActions(chatInputState)
            ChatMenuType.MainMenu(actions)
        }

        menuState.update { it.copy(isVisible = true, type = type) }
    }

    override fun onCopyUsernameClick() = launchUnit {
        val displayModel = chatDisplay.awaitLoaded()
        clipboardService.setPrimaryClip(displayModel.username)
        onCloseMenu()
    }

    override fun onUnreadMessageVisible(message: ChatMessageUiModel) = launchUnit {
        lastReadMessageTimestamp.update { max(it, message.timestamp) }
    }

    override fun onClearEdit() {
        messageInputState.update { it.copy(relation = InputMessageRelation.None, inputMessage = "") }
    }

    override fun onMessageAction(action: MessageAction) {
        when (action) {
            is MessageAction.LongPress -> {
                messagePopupSection.update {
                    MessagePopUpSelection.actions(action.message.id)
                }
            }

            is MessageAction.Press -> handleMessagePress(action.message)

            is MessageAction.Reply -> launchUnit {
                replyToMessage(action.message)
                messagePopupSection.update { null }
            }

            is MessageAction.Copy -> {
                clipboardService.setPrimaryClip(action.text)
                messagePopupSection.update { null }
            }

            is MessageAction.Edit -> {
                startEditMessage(action.message.id, action.text)
                messagePopupSection.update { null }
            }

            is MessageAction.Reaction -> {
                toggleReaction(action.message.id, action.emoji)
                messagePopupSection.update { null }
            }

            is MessageAction.ShowReactionDetails -> {
                messagePopupSection.update { MessagePopUpSelection.reactionDetails(action.message.id) }
            }

            is MessageAction.ReplyPreviewTap -> scrollToMessage(action.messageId)
            is MessageAction.ViewEditHistory -> {
                messagePopupSection.update { null }
                showMessageHistory(action.message.id)
            }

            is MessageAction.DismissActionMenu -> {
                messagePopupSection.update { null }
            }
        }
    }

    override fun onAcceptChatRequest() = launchUnit {
        val contactAccountId = chatId.contactOrNull()?.contactAccountId ?: return@launchUnit

        chatRequestAnswerProgress.value = ChatRequestAnswerProgress.Accepting

        interactor.acceptIncomingRequest(contactAccountId)
            .onFailure {
                chatRequestAnswerProgress.value = ChatRequestAnswerProgress.None
                showPresentationError(it.asChatRequestError().toPresentationError())
            }
    }

    override fun onDeclineChatRequest() = launchUnit {
        val contactAccountId = chatId.contactOrNull()?.contactAccountId ?: return@launchUnit

        chatRequestAnswerProgress.value = ChatRequestAnswerProgress.Declining

        interactor.declineIncomingRequest(contactAccountId)
            .onSuccess { router.back() }
            .onFailure {
                chatRequestAnswerProgress.value = ChatRequestAnswerProgress.None
                showPresentationError(it.asChatRequestError().toPresentationError())
            }
    }

    override fun onAttachClick() = launchUnit {
        val cameraPermissionGranted = permissionAsker.askPermission(Manifest.permission.CAMERA) == PermissionResult.GRANTED
        attachmentExecutor.setCameraAllowed(cameraPermissionGranted)

        attachmentExecutor.execute()
            .onSuccess { attachmentResult ->
                if (attachmentResult == null) return@onSuccess

                val inputState = messageInputState.value
                val text = inputState.inputMessage.trim().takeIf { it.isNotEmpty() }
                val replyToMessageId = (inputState.relation as? InputMessageRelation.Reply)?.preview?.messageId

                messageInputState.clear()

                sendAttachmentMessage(attachmentResult, text, replyToMessageId)
            }
            .onFailure {
                showPresentationError(it.asAttachmentError().toPresentationError())
            }
    }

    private suspend fun sendAttachmentMessage(
        attachmentResult: AttachmentResult,
        text: String?,
        replyToMessageId: ChatMessageId?
    ) {
        interactor.sendAttachmentMessage(
            chatId = chatId,
            contentUri = attachmentResult.uri,
            mimeType = attachmentResult.mimeType,
            fileSize = attachmentResult.size,
            text = text,
            replyToMessageId = replyToMessageId
        ).onFailure {
            showPresentationError(it.asAttachmentError().toPresentationError())
        }
    }

    private suspend fun sendChatRequest(welcomeText: String) {
        val chatRequest = openChatRequest as? OpenChatRequest.StartChatWithContact
        if (chatRequest == null) {
            Timber.e("Expected open chat request to be StartChatWithContact, but got: $openChatRequest")
            return
        }

        interactor.sendContactRequest(openChatRequest, welcomeText)
            .logFailure("Failed to send contact request")
            .onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }

    private suspend fun sendMessageToActiveChat(messageText: String, relation: InputMessageRelation) {
        when (relation) {
            is InputMessageRelation.Edit -> {
                if (messageText != relation.originalText) {
                    interactor.sendEdit(
                        chatId = chatId,
                        messageId = relation.messageId,
                        newText = messageText
                    )
                }
            }

            is InputMessageRelation.Reply -> {
                interactor.sendTextMessage(
                    chatId = chatId,
                    text = messageText,
                    replyToMessageId = relation.preview.messageId
                )
            }

            InputMessageRelation.None -> {
                interactor.sendTextMessage(
                    chatId = chatId,
                    text = messageText,
                    replyToMessageId = null
                )
            }
        }
    }

    private fun jumpToTargetMessageIfNeeded() {
        val messageId = payload.targetMessageId ?: return

        launchUnit {
            chatMessagesState.first { state -> state.dataOrNull?.messages.orEmpty().any { it.id == messageId } }
            scrollToMessage(messageId)
        }
    }

    private fun scrollToMessage(messageId: String) {
        chatMessagesState.value.onLoaded { state ->
            val scrollIndex = state.messages.indexOfFirst { it.id == messageId }.collectionIndexOrNull()

            if (scrollIndex != null) {
                _highlightEvents.trySend(
                    HighlightedMessage(
                        messageId = messageId,
                        scrollIndex = scrollIndex
                    )
                )
            }
        }
    }

    private fun startEditMessage(messageId: ChatMessageId, currentText: String) {
        messageInputState.update {
            it.copy(
                relation = InputMessageRelation.Edit(messageId, currentText),
                inputMessage = currentText
            )
        }
    }

    private fun showMessageHistory(messageId: ChatMessageId) = launchUnit {
        val history = interactor.getEditHistory(chatId, messageId)
            .map { MessageRevisionUiModel(text = it.text, timestamp = it.timestamp) }

        if (history.isEmpty()) return@launchUnit

        val sortedHistory = history.sortedByDescending { it.timestamp }
        val current = sortedHistory.first()
        val pastRevisions = sortedHistory.drop(1).toImmutableList()

        menuState.update {
            it.copy(
                isVisible = true,
                type = ChatMenuType.MessageHistory(
                    current = current,
                    history = pastRevisions
                )
            )
        }
    }

    private fun toggleReaction(messageId: String, emoji: String) = launchUnit {
        val messageReactions = reactions.first()[messageId].orEmpty()
        val existingUserReaction = messageReactions.find { it.origin.isUser() }

        if (existingUserReaction != null) {
            interactor.removeUserReaction(chatId, existingUserReaction)
        }

        if (existingUserReaction?.content?.emoji != emoji) {
            interactor.addUserReaction(
                chatId = chatId,
                messageId = messageId,
                content = ChatMessageReactionContent(emoji)
            )
        }
    }

    private fun replyToMessage(message: ChatMessageUiModel) = launchUnit {
        val reply = messageUiMapper
            .createReplyRelationFor(message, originDisplayResolver.first())
            ?: return@launchUnit

        messageInputState.update { it.copy(relation = reply) }
    }

    private fun handleMessagePress(message: ChatMessageUiModel) {
        when (message) {
            is ChatMessageUiModel.Call -> {
                val withVideo = message.purpose == ChatMessageUiModel.Call.Purpose.VIDEO_CALL

                when (message.state) {
                    is ChatMessageUiModel.Call.State.Ongoing,
                    is ChatMessageUiModel.Call.State.Missed,
                    is ChatMessageUiModel.Call.State.Canceled,
                    is ChatMessageUiModel.Call.State.Declined -> onStartCallClick(withVideo)

                    is ChatMessageUiModel.Call.State.Ringing,
                    is ChatMessageUiModel.Call.State.Ended -> Unit
                }
            }

            else -> Unit
        }
    }

    private fun ChatUserInputState.toChatInputUiState(
        messageInputState: ChatSendMessageInputState,
        answerProgress: ChatRequestAnswerProgress
    ): ChatInputUiState {
        return when (this) {
            is ChatUserInputState.AcceptChatRequest -> ChatInputUiState.AcceptChatRequest(answerProgress)
            is ChatUserInputState.Nothing, ChatUserInputState.UserDeclinedChatRequest -> ChatInputUiState.Hidden
            is ChatUserInputState.PeerLeft -> ChatInputUiState.PeerLeft
            is ChatUserInputState.UnblockUser -> ChatInputUiState.UnblockUser
            is ChatUserInputState.WaitChatRequestApproval -> ChatInputUiState.WaitChatRequestApproval
            is ChatUserInputState.SendChatRequest -> {
                ChatInputUiState.SendMessage(messageInputState.clearRelation(), isChatRequest = true)
            }

            is ChatUserInputState.SendMessage -> {
                ChatInputUiState.SendMessage(
                    messageState = messageInputState,
                    isChatRequest = false,
                    showPayButton = paymentState is ChatUserInputState.SendMessage.Payment.Available,
                    showAttachButton = attachmentsSupported
                )
            }
        }
    }

    private fun markNonDisplayableMessagesAsRead() = launchUnit {
        interactor.markNonDisplayableMessagesAsRead(chatId)
    }

    private fun handleMarkMessagesAsReadTrigger() {
        lastReadMessageTimestamp
            .debounce(500.milliseconds)
            .onEach { timestamp ->
                if (timestamp > 0) interactor.markMessagesAsReadUpToTimestamp(chatId, timestamp)
            }
            .launchIn(this)
    }

    private fun MutableStateFlow<ChatSendMessageInputState>.clear() {
        update { ChatSendMessageInputState() }
    }

    private fun MutableStateFlow<ChatSendMessageInputState>.updateInput(message: String) {
        update { it.copy(inputMessage = message.take(MAX_CHAT_MESSAGE_LENGTH)) }
    }
}

private data class GatedMessages(
    val visibleMessages: List<ChatMessage>,
    val revealingMessageId: ChatMessageId?,
    val revealComplete: Boolean,
)

// Text/RichText render as animated (typing) bubbles that report reveal completion; other content
// (custom renderers, payments, system events) appears instantly and must not gate the reveal.
private val ChatMessage.animatesReveal: Boolean
    get() = content is ChatMessage.Content.Text || content is ChatMessage.Content.RichText
