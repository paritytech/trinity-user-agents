package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.launch
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
    private val screen: Screen,
    private val interactor: ChatFeedInteractor,
    private val resourceResolver: ChatResourceResolver,
    private val productLinkOpener: ChatProductLinkOpener,
) {
    /** What the controller asks of the room screen. */
    interface Screen {
        fun showMenu(type: ChatMenuType)

        fun closeMenu()

        fun showFailure(error: Throwable)
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

    val uiState: StateFlow<ProductRoomUiState> = toolbar
        .map { ProductRoomUiState(toolbar = it) }
        .stateIn(scope, SharingStarted.Eagerly, ProductRoomUiState.None)

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

    /** Runs a product entry of the menu of a message. */
    fun selectMessageMenuAction(messageId: ChatMessageId, actionId: String) {
        val answer = _messageMenu.value as? ProductMessageMenuState.Answered ?: return
        val action = answer.menu?.actions()?.find { it.actionId == actionId } ?: return

        perform(action.handler, ChatRoomUserEvent.MessageActionTriggered(messageId, actionId))
    }

    fun handle(action: ProductRoomAction) {
        when (action) {
            is ProductRoomAction.OpenLink -> openLink(action.url)
            is ProductRoomAction.Toolbar -> pressToolbar(action.actionId)
        }
    }

    private fun pressToolbar(actionId: String) {
        val action = toolbarActions.value.find { it.actionId == actionId } ?: return
        perform(action.handler, ChatRoomUserEvent.ToolbarActionTriggered(actionId))
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

    class Factory @Inject constructor(
        private val interactor: ChatFeedInteractor,
        private val resourceResolver: ChatResourceResolver,
        private val productLinkOpener: ChatProductLinkOpener,
    ) {
        fun <S> create(
            scope: S,
            chatId: ChatId,
            roomPolicy: Flow<ChatRoomPolicy?>,
            feedSnapshot: Flow<ChatFeedSnapshot>,
            screen: Screen,
        ): ChatProductRoomController where S : ComputationalScope, S : MessageDisplay = ChatProductRoomController(
            scope = scope,
            messageDisplay = scope,
            chatId = chatId,
            roomPolicy = roomPolicy,
            feedSnapshot = feedSnapshot,
            screen = screen,
            interactor = interactor,
            resourceResolver = resourceResolver,
            productLinkOpener = productLinkOpener,
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
