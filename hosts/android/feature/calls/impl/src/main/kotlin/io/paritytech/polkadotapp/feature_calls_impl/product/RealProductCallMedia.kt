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
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignalCoder
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealProductCallMedia @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val callStateTracker: CallStateTracker,
    private val coder: PeerSignalCoder,
) : ProductCallMedia {
    private data class Key(val chatId: ChatId, val callId: String)

    private val sessions = mutableMapOf<Key, ProductCallSession>()
    private val pending = mutableMapOf<Key, MutableList<ByteArray>>()

    override fun start(call: ProductMediaCall, signals: ProductCallSignals): Result<Unit> = runCatching {
        check(hasRecordAudioPermission()) { "The microphone permission is not granted" }
        val key = Key(call.chatId, call.callId)
        synchronized(this) {
            if (key in sessions) return@runCatching
            check(callStateTracker.getActiveCall() == null) { "Another call is running" }
            val session = ProductCallSession(call, signals, coder, onClosed = ::forget)
            sessions[key] = session
            pending.remove(key)?.forEach(session::receive)
        }

        val direction = if (call.offerer) CallDirection.Outgoing else CallDirection.Incoming
        val params = CallParams(call.chatId.value.value, call.callId, call.roomName, direction, call.video)
        context.startForegroundService(CallService.startCallIntent(context, params))
        context.startActivity(CallActivity.newIntent(context))
    }

    override fun signal(chatId: ChatId, callId: String, signal: ByteArray) {
        val key = Key(chatId, callId)
        val session = synchronized(this) {
            sessions[key] ?: run {
                val kept = pending.getOrPut(key) { mutableListOf() }
                if (kept.size < PENDING_MAX) kept += signal
                null
            }
        }
        session?.receive(signal)
    }

    override fun stop(chatId: ChatId, callId: String) {
        val session = synchronized(this) {
            pending.remove(Key(chatId, callId))
            sessions[Key(chatId, callId)]
        }
        session?.close()
    }

    fun session(chatId: ChatId, callId: String): ProductCallSession? = synchronized(this) { sessions[Key(chatId, callId)] }

    fun roomName(chatId: ChatId): String? = synchronized(this) { sessions.values.firstOrNull { it.call.chatId == chatId }?.call?.roomName }

    private fun forget(session: ProductCallSession) = synchronized(this) {
        sessions.remove(Key(session.call.chatId, session.call.callId))
    }

    private fun hasRecordAudioPermission(): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private companion object {
        const val PENDING_MAX = 32
    }
}
