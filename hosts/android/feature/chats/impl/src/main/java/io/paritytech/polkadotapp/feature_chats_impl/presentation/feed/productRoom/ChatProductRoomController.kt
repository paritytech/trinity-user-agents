package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.MessagePopUpSelection
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud.ChatReadAloudController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject

/**
 * The parts of a product room that its product declares (02-DESIGN §3.3.7, §3.3.8, §9): the message menu, the
 * toolbar, the participants, who reacted, the footer and its extras. The room view model keeps one, as iOS keeps a
 * `ChatProductRoomPresenter`. Each question to the product has the lookup time of 02-DESIGN §3.6. When the product
 * refuses a question or does not answer it in time, the room uses the host default.
 */
@OptIn(ExperimentalCoroutinesApi::class)
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
    private val router: ChatsRouter,
    callFactory: ProductRoomCallController.Factory,
    opensOverParent: Boolean = false,
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

    private val toolbarActions = roomPolicy
        .map { it?.toolbar.orEmpty() }
        .distinctUntilChanged()
        .stateIn(scope, SharingStarted.Eagerly, emptyList())

    /** The toolbar draws at once with the labels, then again with the icons when they load (P6.2). */
    private val toolbar = toolbarActions
        .transformLatest { actions ->
            emit(actions.toToolbar(icons = emptyMap()))
            if (actions.any { it.icon != null }) emit(actions.toToolbar(actions.resolveIcons()))
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

    private val decorations = ProductRoomDecorations(scope, chatId, roomPolicy, feedSnapshot, additions, participants)

    private val participantSheet = MutableStateFlow<ParticipantSheetUiState?>(null)
    private var participantSheetMenu: ChatParticipantMenuModel? = null
    private var participantSheetJob: Job? = null

    /** A `Custom` footer draws the tree of the product in the footer slot, bounded in height (P6.6). */
    private val customFooter = combine(roomPolicy, flow { emit(interactor.getFooterRendererForChat(chatId)) }) { policy, renderer ->
        renderer.takeIf { policy?.footer == ChatRoomPolicy.Footer.Custom }
    }

    /** The quick replies, the slash commands and the activity line (P6.8). A persona names its activity line. */
    private val extras = combine(roomPolicy, interactor.observeRoomActivity(chatId), participants.personas) { policy, activity, personas ->
        val speakerId = activity?.participantId
        val speaker = speakerId?.let { id -> personas[id]?.let { appearances.persona(id, it) } }
        ProductRoomExtrasUiState(
            suggestions = policy?.suggestions.orEmpty().toImmutableList(),
            commands = policy?.commands.orEmpty().toImmutableList(),
            activity = activity?.text?.let { text -> speaker?.let { "${it.name}: $text" } ?: text }.takeIf { policy != null },
            activitySpeaker = speaker.takeIf { policy != null },
        )
    }

    /** "Read aloud" applies only when the room declares it and the host lists it (07-LATER §9.5). */
    private val readsAloud = roomPolicy
        .map { it?.features?.readAloud == true && ChatHostCapabilities.READ_ALOUD in ChatHostCapabilities.features }
        .distinctUntilChanged()
        .stateIn(scope, SharingStarted.Eagerly, false)

    private val additionsState = combine(
        decorations.decorations,
        decorations.threadRoom,
        readAloud.speaking.map { it == chatId },
        ::Triple,
    )

    /** The call buttons, the ringing banner and the call bar (07-LATER §11.2). The room draws the caller as an author. */
    private val call = callFactory.create(scope, chatId, roomPolicy, participants::appearanceOf, screen::showFailure)

    val uiState: StateFlow<ProductRoomUiState> = combine(
        combine(toolbar, participants.uiState, participantSheet, ::Triple),
        customFooter,
        combine(extras, call.uiState, ::Pair),
        additionsState,
    ) { (toolbar, participants, sheet), customFooter, (extras, call), (decorations, thread, readingAloud) ->
        ProductRoomUiState(
            toolbar = toolbar,
            participants = participants,
            participantSheet = sheet,
            customFooter = customFooter,
            extras = extras,
            decorations = decorations,
            thread = thread,
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
        // A thread room that opens from a link or a notification gets the room of its root under it, so Back returns
        // to the root as after a tap on the thread chip (07-LATER §9.2).
        if (!opensOverParent) scope.launch { openOverParent() }
    }

    private suspend fun openOverParent() {
        val parent = roomPolicy.filterNotNull().first().features?.threadOf ?: return
        if (ChatHostCapabilities.THREADS !in ChatHostCapabilities.features) return
        val extensionId = chatId.extensionOrNull()?.extensionId ?: return
        val parentChatId = ChatId.forExtensionRoom(extensionId, parent.roomId)
        if (parentChatId == chatId) return

        readAloud.stop(chatId)
        router.openChatFeed(ChatFeedPayload.existingChat(parentChatId))
        router.openThreadFeed(ChatFeedPayload.threadChat(chatId))
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
            // The menu of a reasoning row has Copy only, so the product gets no question (07-LATER §9.4).
            if (!asks || message is ChatMessageUiModel.Custom<*> || decorations.isReasoning(message.id)) return@launch

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
            is ProductRoomAction.Toolbar -> pressToolbar(action.actionId)
            ProductRoomAction.ShowParticipants -> showParticipants()
            ProductRoomAction.LoadMoreParticipants -> participants.loadMore()
            is ProductRoomAction.SelectParticipant -> showParticipant(action.participantId)
            is ProductRoomAction.ParticipantSheet -> selectSheetEntry(action.entry)
            is ProductRoomAction.Suggestion -> selectSuggestion(action.suggestionId)
            // The thread room opens on top of this room, so Back returns to the root (07-LATER §9.2).
            is ProductRoomAction.OpenThread -> {
                readAloud.stop(chatId)
                router.openThreadFeed(ChatFeedPayload.threadChat(action.threadChatId))
            }
            is ProductRoomAction.SelectBranch -> decorations.selectBranch(action.messageId, action.index)
            ProductRoomAction.StopReadAloud -> readAloud.stop(chatId)
            is ProductRoomAction.Call -> call.onAction(action.action)
        }
    }

    /** Speaks the plain text of a message of this room. */
    fun readAloud(text: String) {
        if (readsAloud.value) readAloud.speak(chatId, text)
    }

    /**
     * The message menu with the additions of 07-LATER §9: a reasoning row offers Copy only, and a room that declares
     * `read_aloud` adds "Read aloud" for text, also in an `ExtendBuiltIn` menu. A `Custom` menu keeps the entries of
     * the product only.
     */
    fun adjustPopup(state: MessagePopUpUiState?): MessagePopUpUiState? {
        if (state !is MessagePopUpUiState.ActionMenu) return state
        if (decorations.isReasoning(state.message.id)) {
            return state.copy(
                allowedMenuActions = state.allowedMenuActions.filterIsInstance<AllowedMessageMenuAction.Copy>().toImmutableList(),
                canLeaveReactions = false,
                productActionsLoading = false,
            )
        }
        val answer = _messageMenu.value as? ProductMessageMenuState.Answered
        val customMenu = answer != null && answer.messageId == state.message.id && answer.menu is ChatMessageMenuModel.Custom
        val text = decorations.speakableText(state.message.id)?.takeIf { readsAloud.value && !customMenu } ?: return state
        val actions = state.allowedMenuActions
        val position = actions.indexOfFirst { it is AllowedMessageMenuAction.Custom }.takeIf { it >= 0 } ?: actions.size
        val withReadAloud = actions.take(position) + AllowedMessageMenuAction.ReadAloud(text) + actions.drop(position)
        return state.copy(allowedMenuActions = withReadAloud.toImmutableList())
    }

    /** The sheet of a participant: the host entry for its kind, and the entries of `participant_actions_query` (P6.4). */
    private fun showParticipant(participantId: String) {
        participantSheetJob?.cancel()
        participantSheetJob = scope.launch {
            val participant = participants.appearanceOf(participantId)
            val entry = participants.entryOf(participantId)
            val builtIns = ParticipantSheet.builtIns(participantId, entry, interactor.productAppAddress(chatId))
            val asks = entry != null && roomPolicy.first()?.productQueries?.contains(ChatRoomPolicy.ProductQuery.PARTICIPANT_ACTIONS) == true

            participantSheetMenu = null
            participantSheet.value = ParticipantSheetUiState(participant, builtIns.toImmutableList(), loading = asks)
            screen.showMenu(ChatMenuType.ParticipantSheet)
            if (!asks || entry == null) return@launch

            val menu = interactor.roomQuerying(chatId)?.participantMenu(chatId, entry)
                ?.logFailure("The participant sheet keeps the host entry")
                ?.getOrNull()
            participantSheetMenu = menu
            val entries = ParticipantSheet.entries(builtIns, menu, menu?.actions().orEmpty().resolveIcons())
            participantSheet.value = ParticipantSheetUiState(participant, entries.toImmutableList(), loading = false)
        }
    }

    private fun selectSheetEntry(entry: ParticipantSheetEntry) {
        val participantId = participantSheet.value?.participant?.participantId ?: return
        when (entry) {
            is ParticipantSheetEntry.CopyAddress -> {
                clipboardService.setPrimaryClip(entry.address)
                screen.closeMenu()
            }

            is ParticipantSheetEntry.OpenProduct -> openLink(entry.address)

            is ParticipantSheetEntry.Product -> {
                val action = participantSheetMenu?.actions()?.find { it.actionId == entry.actionId } ?: return
                if (action.handler == ChatActionHandlerModel.Callback) screen.closeMenu()
                perform(action.handler, ChatRoomUserEvent.ParticipantActionTriggered(participantId, entry.actionId))
            }
        }
    }

    /** A quick reply with `sends_text` sends its text as a message of the user. Another one sends `SuggestionSelected` only. */
    private fun selectSuggestion(suggestionId: String) {
        scope.launch {
            val suggestion = roomPolicy.first()?.suggestions?.find { it.suggestionId == suggestionId } ?: return@launch
            if (suggestion.sendsText) {
                screen.showOutcome(interactor.sendTextMessage(chatId, suggestion.text, replyToMessageId = null))
            } else {
                interactor.sendUserEvent(chatId, ChatRoomUserEvent.SuggestionSelected(suggestionId)).onFailure(screen::showFailure)
            }
        }
    }

    private fun showParticipants() {
        screen.showMenu(ChatMenuType.Participants)
        if (participants.uiState.value.list.participants.isEmpty()) participants.loadMore()
    }

    private fun pressToolbar(actionId: String) {
        val action = toolbarActions.value.find { it.actionId == actionId } ?: return
        perform(action.handler, ChatRoomUserEvent.ToolbarActionTriggered(actionId))
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
    @JvmName("resolveMenuIcons")
    private suspend fun List<ChatProductActionModel>.resolveIcons(): Map<String, Any> =
        resolveIcons(map { it.actionId to it.icon })

    @JvmName("resolveToolbarIcons")
    private suspend fun List<ChatToolbarActionModel>.resolveIcons(): Map<String, Any> =
        resolveIcons(map { it.actionId to it.icon })

    private suspend fun resolveIcons(icons: List<Pair<String, ChatResourceModel?>>): Map<String, Any> =
        icons.mapNotNull { (actionId, icon) ->
            icon ?: return@mapNotNull null
            resourceResolver.resolve(icon).getOrNull()?.let { actionId to it.model }
        }.toMap()

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
        private val router: ChatsRouter,
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
            opensOverParent: Boolean = false,
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
            router = router,
            callFactory = callFactory,
            opensOverParent = opensOverParent,
        )
    }
}

internal fun List<ChatToolbarActionModel>.toToolbar(icons: Map<String, Any>): ProductToolbarUiModel {
    val items = map { ProductToolbarItemUiModel(it.actionId, it.label, icons[it.actionId]) }
    return ProductToolbarUiModel(
        inline = items.take(ProductToolbarUiModel.INLINE_COUNT).toImmutableList(),
        overflow = items.drop(ProductToolbarUiModel.INLINE_COUNT).toImmutableList(),
    )
}

private fun ChatMessageMenuModel.actions(): List<ChatProductActionModel> = when (this) {
    is ChatMessageMenuModel.Custom -> actions
    is ChatMessageMenuModel.ExtendBuiltIn -> actions
}
