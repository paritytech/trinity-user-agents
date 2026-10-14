@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_calls_impl.service

import io.paritytech.polkadotapp.common.utils.currentTimestampFlow
import io.paritytech.polkadotapp.feature_calls_api.domain.ExternalCallSignaling
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_calls_api.domain.models.isTerminal
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioRouteController
import io.paritytech.polkadotapp.feature_calls_impl.signaling.asPeerChannelSignaling
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelFactory
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.PeerChannelConnectionState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoDegradationPreference
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoEncodingProfile
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.isTerminal
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class CallSessionManager @Inject constructor(
    private val externalCallSignaling: ExternalCallSignaling,
    private val callStateHolder: CallStateHolder,
    private val callAudioRouteController: CallAudioRouteController,
    private val peerChannelFactory: PeerChannelFactory,
) {
    private val peerChannel = MutableStateFlow<PeerChannel?>(null)

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

        val signaling = externalCallSignaling.asPeerChannelSignaling(chatId, offerId, withVideo)

        sessionScope.launch {
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

            observeStatus(sessionScope, channel, chatId, offerId, callDirection, onTerminated)
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
        callAudioRouteController.stop()
        peerChannel.value?.dispose()
        peerChannel.value = null
        Timber.i("endSession: complete")
    }

    suspend fun sendCloseSignal() {
        val activeCall = callStateHolder.getActiveCall()
        if (activeCall == null) {
            Timber.w("sendCloseSignal: no active call, skipping")
            return
        }
        Timber.i("sendCloseSignal: offerId=${activeCall.offerId}")
        declineOffer(activeCall.offerId, activeCall.chatId)
    }

    suspend fun declineOffer(offerId: OfferId, chatId: ChatId) {
        externalCallSignaling.sendCloseSignal(offerId, chatId)
    }

    context(scope: CoroutineScope)
    fun observeIncomingCallCanceled(chatId: ChatId, offerId: OfferId, onCanceled: () -> Unit) = scope.launch {
        Timber.i("observeIncomingCallClose: waiting for close signal, offerId=$offerId")
        externalCallSignaling.subscribeIncomingCloseSignal(offerId, chatId).first()
        Timber.i("observeIncomingCallClose: received close signal")
        callStateHolder.updateStatus(CallStatus.Ended)
        onCanceled()
    }

    private fun observeStatus(
        sessionScope: CoroutineScope,
        channel: PeerChannel,
        chatId: ChatId,
        offerId: OfferId,
        callDirection: CallDirection,
        onTerminated: () -> Unit,
    ) {
        var connectedTimestamp: Long? = null

        val closeSignals = externalCallSignaling.subscribeIncomingCloseSignal(offerId, chatId)
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
                    CallDirection.Outgoing -> externalCallSignaling.observeOfferReadStatus(offerId)
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
}
