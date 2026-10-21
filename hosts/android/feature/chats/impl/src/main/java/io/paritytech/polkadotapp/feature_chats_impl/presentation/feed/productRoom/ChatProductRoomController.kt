package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.markdown.markdownPlainText
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.MessagePopUpSelection
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud.ChatReadAloudController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject

/**
 * The parts of a product room that its product declares (02-DESIGN §3.3.7, §3.3.8, §9): the message menu, the
 * participants, who reacted and the footer. The room view model keeps one, as iOS keeps a
 * `ChatProductRoomPresenter`. Each question to the product has the lookup time of 02-DESIGN §3.6. When the product
 * refuses a question or does not answer it in time, the room uses the host default.
 */
class ChatProductRoomController(
    private val scope: ComputationalScope,
    private val messageDisplay: MessageDisplay,
    private val chatId: ChatId,
    private val roomPolicy: Flow<ChatRoomPolicy?>,
    private val feedSnapshot: Flow<ChatFeedSnapshot>,
    origins: Flow<MessageOriginDisplayResolver>,
    private val screen: Screen,
    private val interactor: ChatFeedInteractor,
    private val resourceResolver: ChatResourceResolver,
    private val productLinkOpener: ChatProductLinkOpener,
    private val appearances: ChatParticipantAppearanceFactory,
    private val clipboardService: ClipboardService,
    additions: ProductRoomAdditionsFeed,
    private val readAloud: ChatReadAloudController,
    callFactory: ProductRoomCallController.Factory,
) {
    /** What the controller asks of the room screen. */
    interface Screen {
        fun showPopup(selection: MessagePopUpSelection)

        fun showMenu(type: ChatMenuType)

        fun closeMenu()

        fun showFailure(error: Throwable)

        fun showOutcome(outcome: ChatSourceActionOutcome)

        /** Closes the room screen. */
        fun leave()
    }

    private val participants = ProductRoomParticipants(
        scope = scope,
        chatId = chatId,
        roomPolicy = roomPolicy,
        feedSnapshot = feedSnapshot,
        origins = origins,
        appearances = appearances,
        personas = additions.personas(chatId),
        querying = { interactor.roomQuerying(chatId) },
    )

    /** The newest snapshot, for the menu of a message. */
    private val snapshot: StateFlow<ChatFeedSnapshot?> = feedSnapshot.stateIn(scope, SharingStarted.Eagerly, null)

    private val participantSheet = MutableStateFlow<ParticipantSheetUiState?>(null)
    private var participantSheetJob: Job? = null

    /** A `Custom` footer draws the tree of the product in the footer slot, bounded in height (P6.6). */
    private val customFooter = combine(roomPolicy, flow { emit(interactor.getFooterRendererForChat(chatId)) }) { policy, renderer ->
        renderer.takeIf { policy?.footer == ChatRoomPolicy.Footer.Custom }
    }

    /** "Read aloud" applies only when the room declares it and the host lists it (07-LATER §9.5). */
    private val readsAloud = roomPolicy
        .map { it?.features?.readAloud == true && ChatHostCapabilities.READ_ALOUD in ChatHostCapabilities.features }
        .distinctUntilChanged()
        .stateIn(scope, SharingStarted.Eagerly, false)

    private val readingAloud = readAloud.speaking.map { it == chatId }

    /** The call buttons, the ringing banner and the call bar (07-LATER §11.2). The room draws the caller as an author. */
    private val call = callFactory.create(scope, chatId, roomPolicy, participants::appearanceOf, screen::showFailure)

    val uiState: StateFlow<ProductRoomUiState> = combine(
        participants.uiState,
        participantSheet,
        customFooter,
        call.uiState,
        readingAloud,
    ) { participants, sheet, customFooter, call, readingAloud ->
        ProductRoomUiState(
            participants = participants,
            participantSheet = sheet,
            customFooter = customFooter,
            readingAloud = readingAloud,
            call = call,
        )
    }.stateIn(scope, SharingStarted.Eagerly, ProductRoomUiState.None)

    private val _reactionDetails = MutableStateFlow<MessagePopUpUiState.ReactionsDetails?>(null)

    /** Who reacted to a message, as the product answered it. */
    val reactionDetails: StateFlow<MessagePopUpUiState.ReactionsDetails?> = _reactionDetails.asStateFlow()

    @Volatile
    private var reactionDetailsNotAvailable = false

    init {
        // A new configuration of the room can serve who reacted again (S47).
        scope.launch {
            roomPolicy.map { it?.configGeneration }.distinctUntilChanged().collect { reactionDetailsNotAvailable = false }
        }
        // A room that its product deletes, or a room of a product that the user uninstalls, closes its screen (02-DESIGN §5.5).
        scope.launch {
            val removed = interactor.observeRoomExists(chatId).dropWhile { exists -> !exists }.firstOrNull { exists -> !exists }
            if (removed != null) screen.leave()
        }
    }

    private val _messageMenu = MutableStateFlow<ProductMessageMenuState?>(null)
    val messageMenu: StateFlow<ProductMessageMenuState?> = _messageMenu.asStateFlow()
    private var messageMenuJob: Job? = null

    /** In a room that asks its product for message menus, the menu shows a loading row until the answer (P6.1). */
    fun askMessageMenu(message: ChatMessageUiModel) {
        messageMenuJob?.cancel()
        _messageMenu.value = null

        messageMenuJob = scope.launch {
            val policy = roomPolicy.first()
            val asks = policy != null && ChatRoomPolicy.ProductQuery.MESSAGE_ACTIONS in policy.productQueries
            if (!asks || message is ChatMessageUiModel.Custom<*>) return@launch

            _messageMenu.value = ProductMessageMenuState.Loading(message.id)

            val snapshot = feedSnapshot.first()
            val menu = snapshot.messages.find { it.id == message.id }?.let { stored ->
                interactor.productMessageMenu(chatId, stored, snapshot).logFailure("The room uses the host menu").getOrNull()
            }
            _messageMenu.value = ProductMessageMenuState.Answered(message.id, menu, menu?.actions().orEmpty().resolveIcons())
        }
    }

    /** Who reacted to a message: the rows that the host keeps, or the answer of `reactions_query` (P6.5). */
    fun showReactionDetails(messageId: ChatMessageId) {
        scope.launch {
            when (roomPolicy.first().reactionDetailsRoute(reactionDetailsNotAvailable)) {
                ReactionDetailsRoute.LOCAL_ROWS -> screen.showPopup(MessagePopUpSelection.reactionDetails(messageId))
                ReactionDetailsRoute.PRODUCT -> askReactionDetails(messageId)
                ReactionDetailsRoute.NONE -> Unit
            }
        }
    }

    private suspend fun askReactionDetails(messageId: ChatMessageId) {
        val querying = interactor.roomQuerying(chatId) ?: return
        querying.reactions(chatId, messageId)
            .onSuccess { reactions ->
                val timestamp = feedSnapshot.first().messages.find { it.id == messageId }?.timestamp ?: 0L
                val participantsById = participants.appearancesOf(reactions.map { it.participantId })
                _reactionDetails.value = reactions.toReactionDetails(messageId, timestamp, participantsById)
                screen.showPopup(MessagePopUpSelection.reactionDetails(messageId))
            }
            .onFailure { error ->
                // `NotAvailable` is no failure: the room hides who reacted for the rest of the screen.
                if (error is ProductChatQueryException.NotAvailable) reactionDetailsNotAvailable = true
                Timber.d(error, "The room does not list who reacted")
            }
    }

    /** Runs a product entry of the menu of a message. */
    fun selectMessageMenuAction(messageId: ChatMessageId, actionId: String) {
        val answer = _messageMenu.value as? ProductMessageMenuState.Answered ?: return
        val action = answer.menu?.actions()?.find { it.actionId == actionId } ?: return

        perform(action.handler, ChatRoomUserEvent.MessageActionTriggered(messageId, actionId))
    }

    fun handle(action: ProductRoomAction) {
        when (action) {
            is ProductRoomAction.OpenLink -> openLink(action.url)
            is ProductRoomAction.ConfirmLink -> confirmMessageLink(action.url)
            ProductRoomAction.ShowParticipants -> showParticipants()
            ProductRoomAction.LoadMoreParticipants -> participants.loadMore()
            is ProductRoomAction.SelectParticipant -> showParticipant(action.participantId)
            is ProductRoomAction.ParticipantSheet -> selectSheetEntry(action.entry)
            ProductRoomAction.StopReadAloud -> readAloud.stop(chatId)
            is ProductRoomAction.Call -> call.onAction(action.action)
        }
    }

    /** Speaks the plain text of a message of this room. */
    fun readAloud(text: String) {
        if (readsAloud.value) readAloud.speak(chatId, text)
    }

    /**
     * The message menu with "Read aloud" (07-LATER §9.5): a room that declares `read_aloud` adds it for text, also in an
     * `ExtendBuiltIn` menu. A `Custom` menu keeps the entries of the product only.
     */
    fun adjustPopup(state: MessagePopUpUiState?): MessagePopUpUiState? {
        if (state !is MessagePopUpUiState.ActionMenu) return state
        val answer = _messageMenu.value as? ProductMessageMenuState.Answered
        val customMenu = answer != null && answer.messageId == state.message.id && answer.menu is ChatMessageMenuModel.Custom
        val text = speakableText(state.message.id)?.takeIf { readsAloud.value && !customMenu } ?: return state
        val actions = state.allowedMenuActions
        val position = actions.indexOfFirst { it is AllowedMessageMenuAction.Custom }.takeIf { it >= 0 } ?: actions.size
        val withReadAloud = actions.take(position) + AllowedMessageMenuAction.ReadAloud(text) + actions.drop(position)
        return state.copy(allowedMenuActions = withReadAloud.toImmutableList())
    }

    /** The sheet of a participant: the host entry for its kind (P6.4). */
    private fun showParticipant(participantId: String) {
        participantSheetJob?.cancel()
        participantSheetJob = scope.launch {
            val participant = participants.appearanceOf(participantId)
            val entry = participants.entryOf(participantId)
            val builtIns = ParticipantSheet.builtIns(participantId, entry, interactor.productAppAddress(chatId))
            participantSheet.value = ParticipantSheetUiState(participant, builtIns.toImmutableList())
            screen.showMenu(ChatMenuType.ParticipantSheet)
        }
    }

    private fun selectSheetEntry(entry: ParticipantSheetEntry) {
        when (entry) {
            is ParticipantSheetEntry.CopyAddress -> {
                clipboardService.setPrimaryClip(entry.address)
                screen.closeMenu()
            }

            is ParticipantSheetEntry.OpenProduct -> openLink(entry.address)
        }
    }

    private fun showParticipants() {
        screen.showMenu(ChatMenuType.Participants)
        if (participants.uiState.value.list.participants.isEmpty()) participants.loadMore()
    }

    /** As on iOS, a link of a message opens only with an `http` or `https` address, and only after the user confirms it. */
    private fun confirmMessageLink(url: String) {
        val scheme = url.substringBefore(':', missingDelimiterValue = "").lowercase()
        if (scheme !in MESSAGE_LINK_SCHEMES) {
            Timber.w("A message link with the scheme '%s' does not open", scheme)
            return
        }
        screen.showMenu(ChatMenuType.ProductLinkConfirmation(url))
    }

    private fun openLink(url: String) {
        screen.closeMenu()
        scope.launch {
            with(scope) { with(messageDisplay) { productLinkOpener.open(url) } }
        }
    }

    /** A link opens after the user confirms it. A callback sends [event] to the product (02-DESIGN §3.3.8). */
    private fun perform(handler: ChatActionHandlerModel, event: ChatRoomUserEvent) {
        when (handler) {
            is ChatActionHandlerModel.Deeplink -> screen.showMenu(ChatMenuType.ProductLinkConfirmation(handler.url))
            ChatActionHandlerModel.Callback -> scope.launch {
                interactor.sendUserEvent(chatId, event).onFailure(screen::showFailure)
            }
        }
    }

    /** The image models of the icons of product entries, by action id. An icon that does not load has none. */
    private suspend fun List<ChatProductActionModel>.resolveIcons(): Map<String, Any> =
        mapNotNull { action ->
            val icon = action.icon ?: return@mapNotNull null
            resourceResolver.resolve(icon).getOrNull()?.let { action.actionId to it.model }
        }.toMap()

    /** The plain text that "Read aloud" speaks for a message: `Text`, and `RichText` without its Markdown marks. */
    private fun speakableText(messageId: ChatMessageId): String? {
        val message = snapshot.value?.messages?.find { it.id == messageId } ?: return null
        val text = when (val content = message.content) {
            is ChatMessage.Content.Text -> content.text
            is ChatMessage.Content.RichText -> content.text?.let(::markdownPlainText)
            is ChatMessage.Content.ProductBody -> (content.body as? ProductMessageBody.RichText)?.text?.let(::markdownPlainText)
            else -> null
        }
        return text?.takeIf { it.isNotBlank() }
    }

    private companion object {
        val MESSAGE_LINK_SCHEMES = setOf("http", "https")
    }

    class Factory @Inject constructor(
        private val interactor: ChatFeedInteractor,
        private val resourceResolver: ChatResourceResolver,
        private val productLinkOpener: ChatProductLinkOpener,
        private val appearances: ChatParticipantAppearanceFactory,
        private val clipboardService: ClipboardService,
        private val additions: ProductRoomAdditionsFeed,
        private val readAloud: ChatReadAloudController,
        private val callFactory: ProductRoomCallController.Factory,
    ) {
        @Suppress("LongParameterList")
        fun <S> create(
            scope: S,
            chatId: ChatId,
            roomPolicy: Flow<ChatRoomPolicy?>,
            feedSnapshot: Flow<ChatFeedSnapshot>,
            origins: Flow<MessageOriginDisplayResolver>,
            screen: Screen,
        ): ChatProductRoomController where S : ComputationalScope, S : MessageDisplay = ChatProductRoomController(
            scope = scope,
            messageDisplay = scope,
            chatId = chatId,
            roomPolicy = roomPolicy,
            feedSnapshot = feedSnapshot,
            origins = origins,
            screen = screen,
            interactor = interactor,
            resourceResolver = resourceResolver,
            productLinkOpener = productLinkOpener,
            appearances = appearances,
            clipboardService = clipboardService,
            additions = additions,
            readAloud = readAloud,
            callFactory = callFactory,
        )
    }
}

private fun ChatMessageMenuModel.actions(): List<ChatProductActionModel> = when (this) {
    is ChatMessageMenuModel.Custom -> actions
    is ChatMessageMenuModel.ExtendBuiltIn -> actions
}
