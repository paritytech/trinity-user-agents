package io.paritytech.polkadotapp.feature_calls_impl.product

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallMedia
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallSignals
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductMediaCall
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_impl.models.CallParams
import io.paritytech.polkadotapp.feature_calls_impl.presentation.CallActivity
import io.paritytech.polkadotapp.feature_calls_impl.service.CallService
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRegistration
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignalCoder
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealProductCallMedia @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val callStateTracker: CallStateTracker,
    private val coder: PeerSignalCoder,
    private val feedListeners: ProductFeedListeners,
) : ProductCallMedia {
    private data class Key(val chatId: ChatId, val callId: String)

    private val sessions = MutableStateFlow<Map<Key, ProductCallSession>>(emptyMap())
    private val offerWatches = mutableMapOf<Key, ProductFeedRegistration>()
    private val pending = mutableMapOf<Key, MutableList<ByteArray>>()

    // The outgoing calls whose call screen the host opened already, under the call id it gave the product, and the calls
    // that ended on this device before the product started their media.
    private val placed = mutableSetOf<Key>()
    private val abandoned = LinkedHashSet<Key>()

    /** The host opened the call screen of an outgoing call and asks the product to place it under [callId]. */
    fun expectOutgoing(chatId: ChatId, callId: String) = synchronized(this) {
        placed += Key(chatId, callId)
    }

    /** The call ended on this device before its media started. A later media start of it is refused. */
    fun abandon(chatId: ChatId, callId: String) = synchronized(this) {
        val key = Key(chatId, callId)
        placed -= key
        abandoned += key
        if (abandoned.size > ABANDONED_MAX) abandoned.remove(abandoned.first())
    }

    override fun start(call: ProductMediaCall, signals: ProductCallSignals): Result<Unit> = runCatching {
        check(hasRecordAudioPermission()) { "The microphone permission is not granted" }
        val key = Key(call.chatId, call.callId)
        val opensScreen = synchronized(this) {
            if (key in sessions.value) return@runCatching
            check(key !in abandoned) { "The call ended on this device" }
            // An incoming call starts only while the user is in the call that they answered; the call service waits for it.
            // An outgoing call that the host placed starts in the call whose screen it opened.
            val activeCall = callStateTracker.getActiveCall()
            if (call.offerer) {
                check(activeCall == null || (key in placed && activeCall.offerId == call.callId)) { "Another call is running" }
            } else {
                check(activeCall?.offerId == call.callId) { "The user is not in this call" }
            }
            val session = ProductCallSession(call, signals, coder, onClosed = ::forget)
            sessions.value += key to session
            if (call.offerer) offerWatches[key] = watchOffer(session)
            pending.remove(key)?.forEach(session::receive)
            call.offerer && !placed.remove(key)
        }

        // An outgoing call that the host did not place opens the call screen as the native chat opens it.
        if (opensScreen) {
            val params = CallParams(call.chatId.value.value, call.callId, call.roomName, CallDirection.Outgoing, call.video)
            context.startForegroundService(CallService.startCallIntent(context, params))
            context.startActivity(CallActivity.newIntent(context))
        }
    }

    override fun signal(chatId: ChatId, callId: String, signal: ByteArray) {
        val key = Key(chatId, callId)
        val session = synchronized(this) {
            sessions.value[key] ?: run {
                val kept = pending.getOrPut(key) { mutableListOf() }
                if (kept.size < PENDING_MAX) kept += signal
                null
            }
        }
        session?.receive(signal)
    }

    override fun stop(chatId: ChatId, callId: String) {
        val session = synchronized(this) {
            placed.remove(Key(chatId, callId))
            pending.remove(Key(chatId, callId))
            sessions.value[Key(chatId, callId)]
        }
        session?.close()
    }

    fun session(chatId: ChatId, callId: String): ProductCallSession? = sessions.value[Key(chatId, callId)]

    /** The session of [callId] once its product starts it. */
    fun observeSession(chatId: ChatId, callId: String): Flow<ProductCallSession?> =
        sessions.map { it[Key(chatId, callId)] }.distinctUntilChanged()

    private fun forget(session: ProductCallSession) = synchronized(this) {
        val key = Key(session.call.chatId, session.call.callId)
        sessions.value -= key
        offerWatches.remove(key)?.cancel()
    }

    // The offer message has the id of the call. Its status turns Read once a device of the contact acknowledged it.
    private fun watchOffer(session: ProductCallSession): ProductFeedRegistration {
        val callId = session.call.callId
        return feedListeners.watch(session.call.chatId) { _, update ->
            val read = when (update) {
                is ProductFeedUpdateModel.StatusChanged -> update.messageId == callId && update.status.isRead
                is ProductFeedUpdateModel.New -> update.message.messageId == callId && update.message.status.isRead
                is ProductFeedUpdateModel.Replaced -> update.message.messageId == callId && update.message.status.isRead
                else -> false
            }
            if (read) session.markOfferRead()
        }
    }

    private val ProductMessageStatusModel.isRead: Boolean
        get() = this == ProductMessageStatusModel.READ || this == ProductMessageStatusModel.DELIVERED

    private fun hasRecordAudioPermission(): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private companion object {
        const val PENDING_MAX = 32
        const val ABANDONED_MAX = 32
    }
}
