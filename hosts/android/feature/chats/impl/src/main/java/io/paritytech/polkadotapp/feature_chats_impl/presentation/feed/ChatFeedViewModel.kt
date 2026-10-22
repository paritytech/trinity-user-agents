package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import android.Manifest
import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.awaitLoaded
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
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.computeChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
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
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
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
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ProductRoomMedia
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
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.math.max
import kotlin.time.Duration.Companion.milliseconds
import io.paritytech.polkadotapp.common.R as RCommon

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
    callStateTracker: CallStateTracker,
    productRoomMediaFactory: ProductRoomMedia.Factory,
) : BaseViewModel(), ChatFeedContract {
    private val payload: ChatFeedPayload = savedStateHandle.getPayload()

    private val openChatRequest: OpenChatRequest = payload.toOpenChatRequest()
    private val chatId = openChatRequest.computeChatId()

    private val messagePopupSection = MutableStateFlow<MessagePopUpSelection?>(null)
    private val _highlightEvents = OneShotEventChannel<HighlightedMessage>()
    override val highlightEvents = _highlightEvents.receiveAsFlow()
    override val scrollToPosition = MutableSharedFlow<Int>()
    private val lastReadMessageTimestamp = MutableStateFlow<Timestamp>(0)

    // A room of the chat product is a person chat also before the product declares its contact.
    private val isPersonChatRoom = interactor.isPersonChatRoom(chatId)

    /** The contact of a room of the chat product, which the room draws as a person chat. Null in any other chat. */
    private val roomContact = interactor.observeRoomPolicy(chatId)
        .map { it?.contact }
        .distinctUntilChanged()
        .shareInBackground()

    // A person chat of the chat product names its contact once the product declares the contact.
    private val originDisplayResolver = roomContact
        .map { interactor.getMessageOriginDisplayResolver(openChatRequest) }
        .shareInBackground()

    private val feedSnapshot = interactor.subscribeMessages(chatId).shareInBackground()

    /** The media of product messages, and the transfers that the messages show. */
    val productMedia = productRoomMediaFactory.create(this)

    private val reactions = feedSnapshot.map { it.reactions }.distinctUntilChanged().shareInBackground()

    private val revisions = feedSnapshot.map { it.revisions }.distinctUntilChanged().shareInBackground()

    private val customMessageRenderers = flowOf { interactor.getCustomMessageRenderersForChat(chatId) }

    private val animatesMessageReveal = flowOf { interactor.animatesMessageReveal(chatId) }
        .shareInBackground()

    private val revealedUpTo = MutableStateFlow<Timestamp?>(null)

    private val gatedDomainMessages = combine(
        feedSnapshot.map { it.messages },
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
            gatedDomainMessages.map { it.visibleMessages }.onEach(productMedia::track),
            callStateTracker.observeActiveCall().map { state -> state?.takeIf { it.chatId == chatId } }
        ),
        combineToPair(reactions, productMedia.files),
        originDisplayResolver,
        combineToPair(revisions, feedSnapshot.map { it.replyQuotes }.distinctUntilChanged()),
        customMessageRenderers,
    ) { (messages, activeCall), (reactions, mediaFiles), originDisplayResolver, (revisionsMap, replyQuotes), customMessageRenderers ->
        messageUiMapper.map(
            messages = messages,
            originDisplayResolver = originDisplayResolver,
            reactions = reactions,
            revisions = revisionsMap,
            customMessageRenderers = customMessageRenderers,
            activeCall = activeCall,
            productMedia = mediaFiles,
            replyQuotes = replyQuotes,
        )
    }.shareInBackground()

    private val paging = feedSnapshot.map { it.paging }.distinctUntilChanged()

    private val readiness = feedSnapshot.map { it.readiness }.distinctUntilChanged()

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

    // A room of the chat product has the menu of a person chat, not the menu of its product.
    override val menuRenderer = combine(flowOf { interactor.getMenuRendererForChat(chatId) }, roomContact) { renderer, contact ->
        renderer.takeIf { contact == null && !isPersonChatRoom }
    }.stateInBackground(initialValue = null)

    override val toolbarActions = combine(
        chatInputStateType,
        menuRenderer,
        roomContact,
    ) { inputStateType, renderer, contact ->
        val hasRenderer = renderer != null
        val isPersonChat = contact != null || isPersonChatRoom
        menuActionsProvider.getToolbarActions(openChatRequest, inputStateType, addMenuIconForcibly = hasRenderer, isPersonChat = isPersonChat)
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

    // The "New messages" marker waits for the first page of a product-source room, so a cached page does not set it, and
    // for that page to reach the mapped list, which starts empty while the snapshot already holds the messages.
    private val firstNewMessageInfo = combineToPair(messages, feedSnapshot)
        .transformWhile { (messagesList, snapshot) ->
            val ready = snapshot.readiness != ChatRoomReadiness.LOADING &&
                (messagesList.isNotEmpty() || snapshot.messages.isEmpty())
            emit(if (ready) messagesList.firstNewMessageInfo() else null)
            !ready
        }
        .distinctUntilChanged()
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
        paging,
        readiness,
    ) { messagesList, firstNewMessageInfo, unreadCounter, paging, readiness ->
        readiness to ChatMessagesState(
            messages = messagesList,
            unreadCounter = unreadCounter,
            firstNewMessageInfo = firstNewMessageInfo,
            olderPage = paging.older.toUi(),
            newerPage = paging.newer.toUi(),
            readiness = readiness.toUi(),
        )
    }
        // The list stays loading until the first page of a product-source room, so it opens at the first unread message
        // as the native list does.
        .dropWhile { (readiness, _) -> readiness == ChatRoomReadiness.LOADING }
        .map { (_, state) -> state }
        .withLoading()
        .stateIn(CoroutineScope(coroutineContext + dispatchers.computation), SharingStarted.Eagerly, LoadingState.Loading)

    private val feedPaging = ChatFeedPaging(chatId, interactor, chatMessagesState)

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
        leaveWhenRoomIsRemoved()
    }

    override fun onCleared() {
        interactor.setChatInactive()
        interactor.releaseRoom(chatId)
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

        val allRevealed = feedSnapshot.first().messages.none { it.timestamp > message.timestamp && it.animatesReveal }
        if (allRevealed) scrollToPosition.emit(NEWEST_MESSAGE_POSITION)
    }

    private fun seedMessageRevealProgress() = launchUnit {
        revealedUpTo.value = interactor.getLastRevealedTimestamp(chatId) ?: 0L
    }

    // A person chat of the chat product opens the native amount screen too; the chat product carries no coins yet, so
    // that screen refuses the payment when the user confirms it.
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

    override fun onScreenStarted() {
        interactor.openRoom(chatId)
        launch { interactor.resumeOutbox(chatId) }
    }

    override fun onScreenStopped() {
        interactor.closeRoom(chatId)
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
            .onSuccess { closeRoomOnce() }
            .onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }

    override fun onBlockUserRequest() = launchUnit {
        val username = chatDisplay.awaitLoaded().username
        menuState.update { it.copy(type = ChatMenuType.BlockConfirmation(username)) }
    }

    override fun onBlockUserConfirm() = launchUnit {
        interactor.blockUser(chatId)
            .onSuccess { closeRoomOnce() }
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
        interactor.reportMessageSeen(chatId, message.id)
    }

    override fun onLoadOlderMessages() = launchUnit { feedPaging.loadOlder() }

    override fun onLoadNewerMessages() = launchUnit { feedPaging.loadNewer() }

    override fun onShowNewestMessages() = launchUnit {
        feedPaging.showNewest()
        scrollToPosition.emit(NEWEST_MESSAGE_POSITION)
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

            is MessageAction.ReplyPreviewTap -> launchUnit { jumpToMessage(action.messageId) }
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
        if (roomContact.first() != null) return@launchUnit answerProductChatRequest(accept = true)

        val contactAccountId = chatId.contactOrNull()?.contactAccountId ?: return@launchUnit

        chatRequestAnswerProgress.value = ChatRequestAnswerProgress.Accepting

        interactor.acceptIncomingRequest(contactAccountId)
            .onFailure {
                chatRequestAnswerProgress.value = ChatRequestAnswerProgress.None
                showPresentationError(it.asChatRequestError().toPresentationError())
            }
    }

    override fun onDeclineChatRequest() = launchUnit {
        if (roomContact.first() != null) return@launchUnit answerProductChatRequest(accept = false)

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
        ).onSuccess(::showActionOutcome).onFailure {
            showPresentationError(it.asAttachmentError().toPresentationError())
        }
    }

    private suspend fun answerProductChatRequest(accept: Boolean) {
        chatRequestAnswerProgress.value = if (accept) ChatRequestAnswerProgress.Accepting else ChatRequestAnswerProgress.Declining

        interactor.answerProductChatRequest(chatId, accept)
            .onSuccess { if (!accept) closeRoomOnce() }
            .onFailure {
                chatRequestAnswerProgress.value = ChatRequestAnswerProgress.None
                showPresentationError(UnexpectedPresentationError(it))
            }
    }

    // A room of the chat product that the product deletes, such as a declined request or a chat that the user left,
    // closes its screen. The room of any other product stays on screen, as before the chat product.
    private fun leaveWhenRoomIsRemoved() = launchUnit {
        if (!isPersonChatRoom) return@launchUnit
        val removed = interactor.observeRoomExists(chatId).dropWhile { exists -> !exists }.firstOrNull { exists -> !exists }
        if (removed != null) closeRoomOnce()
    }

    // A leave or a decline removes the room as well, and the screen closes once.
    private var roomClosed = false

    private fun closeRoomOnce() {
        if (roomClosed) return
        roomClosed = true
        router.back()
    }

    private suspend fun sendChatRequest(welcomeText: String) {
        // The first message in a room of the chat product with a new contact is the welcome message of its request.
        if (roomContact.first() != null) {
            return showActionOutcome(interactor.sendTextMessage(chatId, welcomeText, replyToMessageId = null, request = true))
        }

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
        val outcome = when (relation) {
            is InputMessageRelation.Edit -> {
                if (messageText == relation.originalText) return
                interactor.sendEdit(chatId = chatId, messageId = relation.messageId, newText = messageText)
            }

            is InputMessageRelation.Reply -> interactor.sendTextMessage(chatId, messageText, replyToMessageId = relation.preview.messageId)
            InputMessageRelation.None -> interactor.sendTextMessage(chatId, messageText, replyToMessageId = null)
        }

        showActionOutcome(outcome)
    }

    /** A product that refuses an action of the user gives the reason (02-DESIGN §13 item 9). */
    private fun showActionOutcome(outcome: ChatSourceActionOutcome) {
        if (outcome !is ChatSourceActionOutcome.Refused) return

        val reason = outcome.reason
        if (reason.isNullOrBlank()) showMessage(RCommon.string.chat_message_action_failed) else showError(reason)
    }

    private fun jumpToTargetMessageIfNeeded() {
        val messageId = payload.targetMessageId ?: return

        launchUnit { jumpToMessage(messageId) }
    }

    private suspend fun jumpToMessage(messageId: ChatMessageId) {
        feedPaging.awaitLoaded(messageId)
        scrollToMessage(messageId)
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
        showActionOutcome(interactor.toggleUserReaction(chatId, messageId, emoji, messageReactions))
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

private fun List<ChatMessageUiModel>.firstNewMessageInfo(): FirstNewMessageInfo? {
    val index = indexOfLast { it.isUnread() }.collectionIndexOrNull() ?: return null
    return FirstNewMessageInfo(index = index, messageId = this[index].id)
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
