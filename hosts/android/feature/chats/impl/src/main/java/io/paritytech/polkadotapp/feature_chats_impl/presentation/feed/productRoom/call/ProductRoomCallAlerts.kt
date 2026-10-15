package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.subscribeIsForeground
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallNotifier
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductRingingNotification
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomEventPublisher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/** The in-app banner of a product room that rings while another screen of the app is in front. */
@Immutable
data class ProductRoomCallBannerUiModel(val chatId: ChatId, val callId: String, val roomName: String, val video: Boolean)

/**
 * The ringing calls of the product rooms outside their room screens (07-LATER §11.2, scenario S73):
 * - a ringing call ends after 60 s, also when the product does not clear it;
 * - an in-app banner shows when another screen of the app is in front;
 * - a `CallStyle` notification shows when the app is in the background.
 *
 * A product rings only while its worker runs, so a room of a closed app does not ring (no `ConnectionService`, D31).
 */
@Singleton
class ProductRoomCallAlerts @Inject constructor(
    private val calls: RealChatRoomCallRepository,
    private val publisher: ChatRoomEventPublisher,
    private val rooms: ReadOnlyChatRoomRepository,
    private val appLifecycle: AppLifecycleObserver,
    private val notifier: ProductCallNotifier,
    private val intents: ProductRoomCallIntents,
) : AppInitializer {
    private val shownRooms = MutableStateFlow<Set<ChatId>>(emptySet())
    private val banner = MutableStateFlow<ProductRoomCallBannerUiModel?>(null)
    private var scope: CoroutineScope? = null

    /** The ringing room that the in-app banner shows, or null. */
    val bannerState: StateFlow<ProductRoomCallBannerUiModel?> = banner.asStateFlow()

    /** The time after which the host clears a ringing call (07-LATER §3.6). */
    internal var ringingLifetime: Duration = RINGING_LIFETIME

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        this.scope = scope
        scope.launch { expireRingingCalls() }
        scope.launch { showAlerts() }
    }

    /** A room screen shows the room: its own banner rings there, so the alerts outside the room stop. */
    fun onRoomShown(chatId: ChatId, shown: Boolean) {
        shownRooms.update { if (shown) it + chatId else it - chatId }
    }

    /** Accept or Decline in the notification of a ringing room. A call that does not ring any more gets nothing. */
    fun answerFromNotification(chatId: ChatId, callId: String, accept: Boolean) {
        val target = scope ?: return
        target.launch {
            val ringing = calls.observeAll().value[chatId] as? ChatRoomCallModel.Ringing
            if (ringing?.callId != callId) return@launch
            notifier.cancelRinging(intents.tag(chatId))
            runCancellableCatching { publisher.publish(chatId, ChatRoomEventModel.CallAnswered(callId, accept)) }
                .onFailure { Timber.w(it, "The answer to the ringing call %s did not reach the product", callId) }
        }
    }

    // Each ringing call gets one timer. A new call id in the room starts a new timer.
    private suspend fun expireRingingCalls() {
        val timers = mutableMapOf<ChatId, Pair<String, Job>>()
        calls.observeAll().collect { all ->
            val ringing = all.ringing()
            timers.entries.removeAll { (chatId, timer) ->
                (ringing[chatId]?.callId != timer.first).also { ended -> if (ended) timer.second.cancel() }
            }
            ringing.forEach { (chatId, call) ->
                if (chatId in timers) return@forEach
                timers[chatId] = call.callId to requireNotNull(scope).launch {
                    delay(ringingLifetime)
                    clearIfRinging(chatId, call.callId)
                }
            }
        }
    }

    private fun clearIfRinging(chatId: ChatId, callId: String) {
        val ringing = calls.observeAll().value[chatId] as? ChatRoomCallModel.Ringing
        if (ringing?.callId == callId) {
            Timber.i("product call alert %s expired %s", chatId.hex(), callId)
            calls.set(chatId, null)
        }
    }

    private suspend fun showAlerts() {
        val notified = mutableMapOf<ChatId, String>()
        combine(calls.observeAll(), appLifecycle.subscribeIsForeground(), shownRooms, ::Triple).collect { (all, foreground, shown) ->
            val ringing = all.ringing()
            notified.entries.removeAll { (chatId, callId) ->
                (foreground || ringing[chatId]?.callId != callId).also { stale ->
                    if (stale) notifier.cancelRinging(intents.tag(chatId))
                }
            }

            if (foreground) {
                val target = ringing.entries.firstOrNull { it.key !in shown }
                ringing.keys.filter { it in shown }.forEach { mark(it, "room ${ringing.getValue(it).callId}") }
                banner.value = target?.let { (chatId, call) ->
                    mark(chatId, "banner ${call.callId}")
                    ProductRoomCallBannerUiModel(chatId, call.callId, roomName(chatId), call.video)
                }
            } else {
                banner.value = null
                ringing.forEach { (chatId, call) ->
                    if (notified[chatId] == call.callId) return@forEach
                    notified[chatId] = call.callId
                    val shownNow = notifier.showRinging(notification(chatId, call))
                    mark(chatId, if (shownNow) "notification ${call.callId}" else "no-notification ${call.callId}")
                }
            }
        }
    }

    private suspend fun notification(chatId: ChatId, call: ChatRoomCallModel.Ringing) = ProductRingingNotification(
        tag = intents.tag(chatId),
        roomName = roomName(chatId),
        video = call.video,
        openIntent = intents.openRoom(chatId),
        answerIntent = intents.answer(chatId, call.callId),
        declineIntent = intents.decline(chatId, call.callId),
    )

    private suspend fun roomName(chatId: ChatId): String =
        runCancellableCatching { rooms.getRoomMetadata(chatId).name }.getOrNull().orEmpty()

    // The E2E launcher reads these lines from logcat, as iOS writes its `call-alert` markers.
    private fun mark(chatId: ChatId, value: String) = Timber.i("product call alert %s %s", chatId.hex(), value)

    private fun Map<ChatId, ChatRoomCallModel>.ringing(): Map<ChatId, ChatRoomCallModel.Ringing> =
        mapNotNull { (chatId, call) -> (call as? ChatRoomCallModel.Ringing)?.let { chatId to it } }.toMap()

    private companion object {
        /** The lifetime of a Media offer, and of the ringing hold of the core (`CHAT_RINGING_HOLD_MS`). */
        val RINGING_LIFETIME = 60.seconds
    }
}

internal fun ChatId.hex(): String = value.value.joinToString(separator = "") { "%02x".format(it) }
