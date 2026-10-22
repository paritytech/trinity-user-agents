@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_calls_impl.service

import io.paritytech.polkadotapp.common.utils.currentTimestampFlow
import io.paritytech.polkadotapp.feature_calls_api.domain.ExternalCallSignaling
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_calls_api.domain.models.isTerminal
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioRouteController
import io.paritytech.polkadotapp.feature_calls_impl.product.ProductCallSession
import io.paritytech.polkadotapp.feature_calls_impl.product.RealProductCallMedia
import io.paritytech.polkadotapp.feature_calls_impl.signaling.asPeerChannelSignaling
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelFactory
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.PeerChannelConnectionState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoDegradationPreference
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoEncodingProfile
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.isTerminal
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class CallSessionManager @Inject constructor(
    private val externalCallSignaling: ExternalCallSignaling,
    private val callStateHolder: CallStateHolder,
    private val callAudioRouteController: CallAudioRouteController,
    private val peerChannelFactory: PeerChannelFactory,
    private val productCallMedia: RealProductCallMedia,
    private val roomCalls: ChatRoomCallRepository,
    private val roomEvents: ChatRoomEventPublisher,
) {
    private val peerChannel = MutableStateFlow<PeerChannel?>(null)
    private var sessionSignaling: ExternalCallSignaling? = null

    fun startSession(
        sessionScope: CoroutineScope,
        chatId: ChatId,
        offerId: OfferId,
        callDirection: CallDirection,
        withVideo: Boolean,
        onTerminated: () -> Unit,
    ) {
        Timber.i("startSession: direction=$callDirection, offerId=$offerId, withVideo=$withVideo")
        peerChannel.value?.dispose()

        with(sessionScope) { callAudioRouteController.start(defaultToSpeaker = withVideo) }

        sessionScope.launch {
            // An answered call of a product room starts once its product starts the media of the call.
            val callSignaling = if (chatId.isExtensionChat()) {
                awaitProductSession(chatId, offerId, callDirection) ?: run {
                    Timber.w("startSession: product call $offerId did not start")
                    // As every native call that ends, the screen shows the end before it closes.
                    callStateHolder.updateStatus(CallStatus.Failed)
                    endProductCallWithoutMedia(chatId, offerId, incoming = callDirection == CallDirection.Incoming)
                    onTerminated()
                    return@launch
                }
            } else {
                externalCallSignaling
            }
            sessionSignaling = callSignaling
            val signaling = callSignaling.asPeerChannelSignaling(chatId, offerId, withVideo)

            val channel = peerChannelFactory.createSingleConnection(
                signaling = signaling,
                mediaConfiguration = MediaConfiguration.AudioVideo(
                    initialCameraEnabled = withVideo,
                    initialMicrophoneEnabled = true,
                    videoProfile = VideoEncodingProfile(
                        captureTargetHeight = 720,
                        captureFps = 30,
                        maxBitrateBps = 2_000_000,
                        degradationPreference = VideoDegradationPreference.BALANCED
                    )
                ),
                scope = sessionScope,
                isInitiator = callDirection == CallDirection.Outgoing,
                logger = PeerConnectionLogger(offerId)
            )

            channel.startConnection()

            observeStatus(sessionScope, callSignaling, channel, chatId, offerId, callDirection, onTerminated)
            observeMediaTracks(sessionScope, channel)
            observeMediaState(sessionScope, channel)

            peerChannel.value = channel
        }
    }

    fun selectAudioDevice(deviceId: Int) {
        callAudioRouteController.selectDevice(deviceId)
    }

    fun endSession() {
        Timber.i("endSession: disposing peer channel, hasChannel=${peerChannel.value != null}")
        // A product call that ends without its close signal, a failed one for example, still ends in the product.
        (sessionSignaling as? ProductCallSession)?.let { session ->
            CoroutineScope(Dispatchers.IO + NonCancellable).launch {
                runCatching { session.sendCloseSignal(session.call.callId, session.call.chatId) }
                    .onFailure { Timber.w(it, "endSession: the product did not take the end of the call") }
            }
        }
        callAudioRouteController.stop()
        peerChannel.value?.dispose()
        peerChannel.value = null
        sessionSignaling = null
        Timber.i("endSession: complete")
    }

    suspend fun sendCloseSignal() {
        val activeCall = callStateHolder.getActiveCall()
        if (activeCall == null) {
            Timber.w("sendCloseSignal: no active call, skipping")
            return
        }
        Timber.i("sendCloseSignal: offerId=${activeCall.offerId}")
        val signaling = sessionSignaling ?: signalingOf(activeCall.chatId, activeCall.offerId)
        if (signaling != null) {
            signaling.sendCloseSignal(activeCall.offerId, activeCall.chatId)
        } else {
            endProductCallWithoutMedia(activeCall.chatId, activeCall.offerId, incoming = activeCall.direction == CallDirection.Incoming)
        }
    }

    suspend fun declineOffer(offerId: OfferId, chatId: ChatId) {
        val signaling = signalingOf(chatId, offerId)
        if (signaling != null) {
            signaling.sendCloseSignal(offerId, chatId)
        } else {
            endProductCallWithoutMedia(chatId, offerId, incoming = true)
        }
    }

    // A product room's call goes through the product, which may have ended it already.
    private fun signalingOf(chatId: ChatId, offerId: OfferId): ExternalCallSignaling? =
        if (chatId.isExtensionChat()) productCallMedia.session(chatId, offerId) else externalCallSignaling

    /**
     * Ends a call of a product room whose media does not run: a ringing call, or an answered one whose media did not
     * start, or an outgoing one that the product did not place. The decline of an incoming call reaches the product also
     * before it has the offer; the end reaches a call it made active. A media start that comes after it is refused.
     */
    private suspend fun endProductCallWithoutMedia(chatId: ChatId, offerId: OfferId, incoming: Boolean) {
        if (!chatId.isExtensionChat()) return
        productCallMedia.abandon(chatId, offerId)
        if (incoming) roomEvents.publish(chatId, ChatRoomEventModel.CallAnswered(offerId, accept = false))
        roomEvents.publish(chatId, ChatRoomEventModel.CallEnded(offerId))
    }

    /**
     * Null when the product clears the call, gives the room another call, or does not start the media in time. An
     * outgoing call that finds another call in its room is a call the product refused, as it is busy.
     */
    private suspend fun awaitProductSession(chatId: ChatId, offerId: OfferId, direction: CallDirection): ProductCallSession? =
        withTimeoutOrNull(PRODUCT_PICKUP_TIMEOUT) {
            var taken = false
            combine(productCallMedia.observeSession(chatId, offerId), roomCalls.observe(chatId), ::Pair)
                .first { (session, call) ->
                    if (call?.callId == offerId) taken = true
                    val refused = direction == CallDirection.Outgoing && call != null && call.callId != offerId
                    session != null || refused || (taken && call?.callId != offerId)
                }.first
        }

    context(scope: CoroutineScope)
    fun observeIncomingCallCanceled(chatId: ChatId, offerId: OfferId, onCanceled: () -> Unit) = scope.launch {
        Timber.i("observeIncomingCallClose: waiting for close signal, offerId=$offerId")
        // A product room rings until its product clears the call or gives the room another call, as a native call rings
        // until its caller ends it. A call that rings from its push stops when the product, once woken, does not take it up.
        if (chatId.isExtensionChat()) {
            val taken = withTimeoutOrNull(PRODUCT_PICKUP_TIMEOUT) { roomCalls.observe(chatId).first { it?.callId == offerId } }
            if (taken != null) roomCalls.observe(chatId).first { it?.callId != offerId }
        } else {
            externalCallSignaling.subscribeIncomingCloseSignal(offerId, chatId).first()
        }
        Timber.i("observeIncomingCallClose: received close signal")
        callStateHolder.updateStatus(CallStatus.Ended)
        onCanceled()
    }

    private fun observeStatus(
        sessionScope: CoroutineScope,
        signaling: ExternalCallSignaling,
        channel: PeerChannel,
        chatId: ChatId,
        offerId: OfferId,
        callDirection: CallDirection,
        onTerminated: () -> Unit,
    ) {
        var connectedTimestamp: Long? = null

        val closeSignals = signaling.subscribeIncomingCloseSignal(offerId, chatId)
            .map { CallStatus.Ended }

        val connectionStatuses = channel.connectionState.flatMapLatest { state ->
            Timber.i("connectionState=$state")
            when {
                state.isTerminal() -> flowOf(CallStatus.Failed)

                // Once Connected has been reached at least once, the call is established
                // from the user's perspective. WebRTC may transiently report Connecting
                // again during media renegotiation (data-channel-first setup) or ICE
                // restarts — those are transport-level concerns, not user-facing state.
                state == PeerChannelConnectionState.Connected || connectedTimestamp != null -> {
                    val start = connectedTimestamp ?: System.currentTimeMillis()
                        .also { connectedTimestamp = it }

                    currentTimestampFlow(1.seconds).map { currentTimestamp ->
                        CallStatus.Connected((currentTimestamp - start).milliseconds)
                    }
                }

                else -> when (callDirection) {
                    CallDirection.Outgoing -> signaling.observeOfferReadStatus(offerId)
                        .map { isRead -> if (isRead) CallStatus.Ringing else CallStatus.Requesting }
                    CallDirection.Incoming -> flowOf(CallStatus.Connecting)
                }
            }
        }

        merge(closeSignals, connectionStatuses)
            .transformWhile { status ->
                emit(status)
                !status.isTerminal()
            }
            .onEach { status ->
                callStateHolder.updateStatus(status)
                if (status.isTerminal()) onTerminated()
            }
            .launchIn(sessionScope)
    }

    private fun observeMediaTracks(sessionScope: CoroutineScope, channel: PeerChannel) {
        channel.mediaTracks
            .onEach { callStateHolder.updateMediaTracks(it) }
            .launchIn(sessionScope)
    }

    private fun observeMediaState(sessionScope: CoroutineScope, channel: PeerChannel) {
        channel.mediaState
            .onEach { callStateHolder.updateMediaState(it) }
            .launchIn(sessionScope)
    }

    suspend fun setLocalCameraEnabled(enabled: Boolean) {
        peerChannel.value?.setLocalCameraEnabled(enabled)
    }

    suspend fun setLocalMicrophoneEnabled(enabled: Boolean) {
        // PeerChannel handles authoritative track-level mute + peer signal + state update.
        peerChannel.value?.setLocalMicrophoneEnabled(enabled)
        // OS-level mute additionally keeps system indicators / audio routing in sync.
        callAudioRouteController.setMicrophoneMute(!enabled)
    }

    private companion object {
        /** How long an answered or pushed call of a product room waits for its product, as long as an offer rings. */
        val PRODUCT_PICKUP_TIMEOUT = 30.seconds
    }
}
