package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantUiModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject

/**
 * The call of one product room screen (07-LATER §11.2, D31): the call buttons that the room declares, the ringing
 * banner, the call bar and the call screen. The product sets the call, and the user answers through the room events.
 * The host changes the call only when it clears a ringing call (`ProductRoomCallAlerts`).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ProductRoomCallController(
    private val scope: CoroutineScope,
    private val chatId: ChatId,
    roomPolicy: Flow<ChatRoomPolicy?>,
    calls: ChatRoomCallRepository,
    private val publisher: ChatRoomEventPublisher,
    private val alerts: ProductRoomCallAlerts,
    private val resolveCaller: suspend (participantId: String) -> ChatParticipantUiModel?,
    private val servedFeatures: Collection<String>,
    private val onFailure: (Throwable) -> Unit,
) {
    private val showsCallScreen = MutableStateFlow(false)

    private val buttons = roomPolicy
        .map { policy -> policy?.features?.calls?.takeIf { ChatHostCapabilities.CALL_BUTTONS in servedFeatures } }
        .map { calls -> calls?.takeIf { it.audio || it.video }?.let { ProductRoomCallUiState.Buttons(it.audio, it.video) } }
        .distinctUntilChanged()

    private val call = calls.observe(chatId).mapLatest { it?.toUi() }

    private val current = call.stateIn(scope, SharingStarted.Eagerly, null)

    val uiState: StateFlow<ProductRoomCallUiState> = combine(buttons, call, showsCallScreen) { buttons, call, screen ->
        ProductRoomCallUiState(
            buttons = buttons,
            call = call,
            showsCallScreen = screen && call is ProductRoomCallUiState.Call.Active && call.hasMedia,
        )
    }.stateIn(scope, SharingStarted.Eagerly, ProductRoomCallUiState.None)

    fun onAction(action: ProductRoomCallAction) {
        when (action) {
            is ProductRoomCallAction.Request -> {
                Timber.d("Product room call: the user asks for a call video=%s", action.video)
                publish(ChatRoomEventModel.CallRequested(action.video))
            }

            is ProductRoomCallAction.Answer -> (current.value as? ProductRoomCallUiState.Call.Ringing)?.let {
                publish(ChatRoomEventModel.CallAnswered(it.callId, action.accept))
            }

            // A call without a session of the Media service is the product's: the product opens its own call page.
            ProductRoomCallAction.Open -> when (val active = current.value) {
                is ProductRoomCallUiState.Call.Active ->
                    if (active.hasMedia) showsCallScreen.value = true else publish(ChatRoomEventModel.CallOpened(active.callId))

                else -> Unit
            }

            ProductRoomCallAction.End -> {
                showsCallScreen.update { false }
                (current.value as? ProductRoomCallUiState.Call.Active)?.let { publish(ChatRoomEventModel.CallEnded(it.callId)) }
            }

            ProductRoomCallAction.CloseScreen -> showsCallScreen.update { false }

            is ProductRoomCallAction.ScreenShown -> alerts.onRoomShown(chatId, action.shown)
        }
    }

    private fun publish(event: ChatRoomEventModel) {
        scope.launch {
            runCancellableCatching { publisher.publish(chatId, event) }.onFailure(onFailure)
        }
    }

    private suspend fun ChatRoomCallModel.toUi(): ProductRoomCallUiState.Call = when (this) {
        is ChatRoomCallModel.Ringing -> {
            val appearance = caller?.let { runCancellableCatching { resolveCaller(it) }.getOrNull() }
            ProductRoomCallUiState.Call.Ringing(
                callId = callId,
                video = video,
                callerName = appearance?.name ?: caller,
                callerAvatar = appearance?.avatar,
            )
        }

        is ChatRoomCallModel.Active -> ProductRoomCallUiState.Call.Active(callId, video, startedAt, hasMedia = mediaSession != null)
    }

    class Factory @Inject constructor(
        private val calls: ChatRoomCallRepository,
        private val publisher: ChatRoomEventPublisher,
        private val alerts: ProductRoomCallAlerts,
    ) {
        fun create(
            scope: CoroutineScope,
            chatId: ChatId,
            roomPolicy: Flow<ChatRoomPolicy?>,
            resolveCaller: suspend (participantId: String) -> ChatParticipantUiModel?,
            onFailure: (Throwable) -> Unit,
            servedFeatures: Collection<String> = ChatHostCapabilities.features,
        ) = ProductRoomCallController(scope, chatId, roomPolicy, calls, publisher, alerts, resolveCaller, servedFeatures, onFailure)
    }
}
