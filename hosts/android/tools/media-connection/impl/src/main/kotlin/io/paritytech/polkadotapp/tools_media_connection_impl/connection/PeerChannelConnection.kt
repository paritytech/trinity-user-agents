package io.paritytech.polkadotapp.tools_media_connection_impl.connection

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.utils.chunked
import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelSignaling
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaTracks
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.PeerChannelConnectionState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoEncodingProfile
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.videoProfile
import io.paritytech.polkadotapp.tools_media_connection_impl.RealDataTransport
import io.paritytech.polkadotapp.tools_media_connection_impl.RealVideoTrack
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import io.paritytech.polkadotapp.tools_media_connection_impl.media.MediaTrackProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.MediaStateSignal
import io.paritytech.polkadotapp.tools_media_connection_impl.models.PeerConnectionCandidate
import io.paritytech.polkadotapp.tools_media_connection_impl.models.PeerConnectionSignal
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.DataChannelMessaging
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.SdpCoder
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.CompoundPeerConnectionObserver
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.SimplePeerConnectionObserver
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.addCandidate
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.applyEncodingProfile
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.awaitRemoteSdpSet
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.preferVideoCodec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.*
import org.webrtc.*
import kotlin.time.Duration.Companion.milliseconds

internal abstract class PeerChannelConnection(
    protected val signaling: PeerChannelSignaling,
    protected val mediaConfiguration: MediaConfiguration,
    private val mediaTrackProvider: MediaTrackProvider,
    private val webRtcCore: WebRtcCore,
    externalRtcConfig: ExternalRtcConfig,
    scope: CoroutineScope,
    protected val logger: PeerConnectionLogger,
) : PeerChannel, CoroutineScope by scope {
    companion object {
        private const val RENEGOTIATION_USE_CASE_ID = "webrtc_renegotiation_internal_use_case"
        private const val MEDIA_STATE_USE_CASE_ID = "webrtc_media_state_use_case"
        private const val ICE_CANDIDATE_POOL_SIZE = 8
        private const val MAX_IPV6_NETWORKS = 1
        private const val PREFERRED_VIDEO_CODEC = "H264"
        private const val MAX_STARTUP_BITRATE_BPS = 1_000_000
    }

    protected val sdpCoder = SdpCoder()
    protected val dataChannelMessaging = DataChannelMessaging(this, logger)
    private val realDataTransport = RealDataTransport(dataChannelMessaging)

    override val dataTransport get() = realDataTransport

    override val connectionState = MutableStateFlow(PeerChannelConnectionState.New)
    override val mediaTracks = MutableStateFlow(MediaTracks())
    override val mediaState = MutableStateFlow(initialMediaState(mediaConfiguration))

    private val localIceCandidatesChannel = Channel<IceCandidate>(Channel.UNLIMITED)
    private val externalIceCandidatesFlow = signaling.subscribeIncomingIceCandidates()

    private val compoundObserver = CompoundPeerConnectionObserver()

    private val baseObserver: PeerConnection.Observer = object : SimplePeerConnectionObserver() {
        override fun onConnectionChange(newState: PeerConnection.PeerConnectionState) {
            logger.log("onConnectionChange: newState=$newState")
            val state = newState.toPeerChannelState()
            connectionState.value = state

            when (state) {
                PeerChannelConnectionState.Disconnected,
                PeerChannelConnectionState.Failed,
                PeerChannelConnectionState.Closed -> {
                    mediaTracks.clearRemoteVideoTrack()
                }

                else -> Unit
            }
        }

        override fun onIceCandidate(iceCandidate: IceCandidate) {
            localIceCandidatesChannel.trySend(iceCandidate)
        }

        override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream?>) {
            receiver.track()?.let { track ->
                if (track is VideoTrack) {
                    mediaTracks.updateRemoteVideoTrack(RealVideoTrack(track, webRtcCore.eglBase.eglBaseContext))
                    logger.log("remote video track added")
                }
            }
        }

        override fun onRemoveTrack(receiver: RtpReceiver?) {
            receiver?.track()?.let { track ->
                if (track is VideoTrack) {
                    mediaTracks.clearRemoteVideoTrack()
                    logger.log("remote video track removed")
                }
            }
        }
    }

    private val iceServers = listOf(
        PeerConnection.IceServer.builder(
            listOf(
                "stun:stun.l.google.com:19302",
                "stun:stun1.l.google.com:19302",
                "stun:stun2.l.google.com:19302",
                "stun:stun3.l.google.com:19302",
                "stun:stun4.l.google.com:19302"
            )
        ).createIceServer()
    ) + externalRtcConfig.turnCredentials.map { turn ->
        PeerConnection.IceServer.builder(turn.url)
            .setUsername(turn.username)
            .setPassword(turn.password)
            .createIceServer()
    }

    private val rtcConfig = PeerConnection.RTCConfiguration(iceServers).apply {
        sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
        iceCandidatePoolSize = ICE_CANDIDATE_POOL_SIZE
        maxIPv6Networks = MAX_IPV6_NETWORKS
    }

    protected val connection: PeerConnection by lazy {
        compoundObserver.addObserver(baseObserver)
        webRtcCore.peerConnectionFactory.createPeerConnection(rtcConfig, compoundObserver)!!
    }

    init {
        dataChannelMessaging
            .state
            .onEach {
                logger.log("Data channel state: state=$it")
            }
            .launchIn(this)

        observeRemoteIceCandidates()
        observeRemoteMediaSignals()

        invokeOnCompletion {
            mediaTracks.clearRemoteVideoTrack()
            localIceCandidatesChannel.close()
            dataChannelMessaging.dispose()
            connection.dispose()
        }
    }

    fun addObserver(observer: PeerConnection.Observer) {
        compoundObserver.addObserver(observer)
    }

    override fun dispose() {
        cancel()
    }

    override suspend fun setLocalCameraEnabled(enabled: Boolean) {
        mediaTrackProvider.setVideoEnabled(enabled)
        mediaState.update { it.copy(localCameraEnabled = enabled) }
        sendMediaSignal(MediaStateSignal.CameraEnabled(enabled))
    }

    override suspend fun setLocalMicrophoneEnabled(enabled: Boolean) {
        mediaTrackProvider.setAudioEnabled(enabled)
        mediaState.update { it.copy(localMicrophoneEnabled = enabled) }
        sendMediaSignal(MediaStateSignal.MicrophoneEnabled(enabled))
    }

    private fun observeRemoteIceCandidates() {
        val externalCandidatesFlow = externalIceCandidatesFlow
            .map { encodedCandidates ->
                sdpCoder.decodeCandidates(encodedCandidates)
            }

        val dataChannelCandidatesFlow = subscribeSignals()
            .filterIsInstance<PeerConnectionSignal.IceCandidates>()
            .map { candidatesSignal ->
                candidatesSignal.candidates.map {
                    IceCandidate(
                        it.sdpMid,
                        it.sdpMLineIndex.toInt(),
                        it.sdp
                    )
                }
            }

        merge(
            externalCandidatesFlow,
            dataChannelCandidatesFlow
        ).onEach { candidates ->
            for (candidate in candidates) {
                logger.log("Received candidate: sdpMid=${candidate.sdpMid}, sdpMLineIndex=${candidate.sdpMLineIndex}")
                runCatching {
                    connection.awaitRemoteSdpSet()
                    connection.addCandidate(candidate)
                }.onFailure {
                    logger.log("Failed to add candidate", it)
                }
            }
        }.launchIn(this)
    }

    private fun observeRemoteMediaSignals() {
        subscribeMediaSignals()
            .onEach { signal ->
                logger.log("Remote media signal: signal=$signal")
                mediaState.update { current ->
                    when (signal) {
                        is MediaStateSignal.CameraEnabled -> current.copy(remoteCameraEnabled = signal.enabled)
                        is MediaStateSignal.MicrophoneEnabled -> current.copy(remoteMicrophoneEnabled = signal.enabled)
                    }
                }
            }
            .launchIn(this)
    }

    protected fun startSendingLocalIceCandidates() {
        localIceCandidatesChannel
            .receiveAsFlow()
            .chunked(4, 500.milliseconds)
            .onEach { candidates ->
                if (candidates.isNotEmpty()) {
                    if (dataChannelMessaging.isOpen()) {
                        logger.log("Sending ${candidates.size} candidates via data channel")
                        sendCandidatesViaDataChannel(candidates)
                    } else {
                        logger.log("Data channel not open, sending ${candidates.size} candidates via external signaling")
                        sendCandidatesViaExternalSignaling(candidates)
                    }
                }
            }
            .launchIn(this)
    }

    protected suspend fun awaitInitialCandidates(): List<IceCandidate> {
        return localIceCandidatesChannel
            .receiveAsFlow()
            .chunked(4, 500.milliseconds)
            .first()
    }

    protected suspend fun initiateLocalMediaTracks() {
        when (mediaConfiguration) {
            MediaConfiguration.None -> return

            is MediaConfiguration.VideoOnly -> {
                val videoTrack = mediaTrackProvider.getOrCreateVideoTrack()
                connection.addTrack(videoTrack)
                connection.preferVideoCodec(webRtcCore.peerConnectionFactory, PREFERRED_VIDEO_CODEC)
                mediaTracks.updateLocalVideoTrack(RealVideoTrack(videoTrack, webRtcCore.eglBase.eglBaseContext))
            }

            is MediaConfiguration.AudioVideo -> {
                val audioTrack = mediaTrackProvider.getOrCreateAudioTrack()
                val videoTrack = mediaTrackProvider.getOrCreateVideoTrack()
                connection.addTrack(audioTrack)
                connection.addTrack(videoTrack)
                connection.preferVideoCodec(webRtcCore.peerConnectionFactory, PREFERRED_VIDEO_CODEC)

                mediaTrackProvider.setAudioEnabled(mediaConfiguration.initialMicrophoneEnabled)
                mediaTrackProvider.setVideoEnabled(mediaConfiguration.initialCameraEnabled)

                mediaTracks.updateLocalVideoTrack(RealVideoTrack(videoTrack, webRtcCore.eglBase.eglBaseContext))
            }
        }
    }

    protected fun applyVideoSenderParams() {
        val profile = mediaConfiguration.videoProfile() ?: return

        runCatching {
            val videoSender = connection.transceivers.firstOrNull {
                it.mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO
            }?.sender ?: return@runCatching

            videoSender.applyEncodingProfile(profile)

            val startBitrateBps = if (profile.maxBitrateBps == VideoEncodingProfile.UNLIMITED_BITRATE_BPS) {
                MAX_STARTUP_BITRATE_BPS
            } else {
                minOf(MAX_STARTUP_BITRATE_BPS, profile.maxBitrateBps)
            }
            connection.setBitrate(null, startBitrateBps, null)
            logger.log("Applied video sender params: maxBitrate=${profile.maxBitrateBps}, startBitrate=$startBitrateBps")
        }.onFailure {
            logger.log("Failed to apply video sender params", it)
        }
    }

    protected suspend fun sendSignal(signal: PeerConnectionSignal) {
        val signalData = BinaryScale.encodeToByteArray<PeerConnectionSignal>(signal)
        dataChannelMessaging.sendMessage(RENEGOTIATION_USE_CASE_ID, signalData)
    }

    protected fun subscribeSignals(): Flow<PeerConnectionSignal> {
        return dataChannelMessaging.subscribeMessages(RENEGOTIATION_USE_CASE_ID)
            .mapNotNull { data ->
                runCatching {
                    BinaryScale.decodeFromByteArray<PeerConnectionSignal>(data)
                }.getOrNull()
            }
    }

    private suspend fun sendMediaSignal(signal: MediaStateSignal) {
        val data = BinaryScale.encodeToByteArray<MediaStateSignal>(signal)
        dataChannelMessaging.sendMessage(MEDIA_STATE_USE_CASE_ID, data)
    }

    private fun subscribeMediaSignals(): Flow<MediaStateSignal> {
        return dataChannelMessaging.subscribeMessages(MEDIA_STATE_USE_CASE_ID)
            .mapNotNull { data ->
                runCatching {
                    BinaryScale.decodeFromByteArray<MediaStateSignal>(data)
                }.getOrNull()
            }
    }

    private suspend fun sendCandidatesViaExternalSignaling(candidates: List<IceCandidate>) {
        val encodedCandidates = sdpCoder.encodeCandidates(candidates)
        signaling.sendIceCandidates(encodedCandidates)
    }

    private suspend fun sendCandidatesViaDataChannel(candidates: List<IceCandidate>) {
        val signal = PeerConnectionSignal.IceCandidates(
            candidates.map {
                PeerConnectionCandidate(
                    sdpMid = it.sdpMid,
                    sdpMLineIndex = it.sdpMLineIndex.toUInt(),
                    sdp = it.sdp
                )
            }
        )

        sendSignal(signal)
    }

    protected fun logNegotiatedVideoCodec(answer: SessionDescription) {
        val lines = answer.description.lines()

        val primaryPayloadType = lines
            .firstOrNull { it.startsWith("m=video") }
            ?.split(" ")
            ?.getOrNull(3)
            ?: return

        val codecName = lines
            .firstOrNull { it.startsWith("a=rtpmap:$primaryPayloadType ") }
            ?.substringAfter("a=rtpmap:$primaryPayloadType ")
            ?.substringBefore("/")
            ?: return

        logger.log("Negotiated video codec: $codecName")
    }
}

private fun initialMediaState(mediaConfiguration: MediaConfiguration): MediaState =
    when (mediaConfiguration) {
        is MediaConfiguration.AudioVideo -> MediaState(
            localCameraEnabled = mediaConfiguration.initialCameraEnabled,
            localMicrophoneEnabled = mediaConfiguration.initialMicrophoneEnabled,
            remoteCameraEnabled = mediaConfiguration.initialCameraEnabled,
            remoteMicrophoneEnabled = mediaConfiguration.initialMicrophoneEnabled,
        )
        is MediaConfiguration.VideoOnly -> MediaState(
            localCameraEnabled = mediaConfiguration.initialCameraEnabled,
            localMicrophoneEnabled = false,
            remoteCameraEnabled = mediaConfiguration.initialCameraEnabled,
            remoteMicrophoneEnabled = false,
        )
        MediaConfiguration.None -> MediaState(
            localCameraEnabled = true,
            localMicrophoneEnabled = false,
            remoteCameraEnabled = true,
            remoteMicrophoneEnabled = false,
        )
    }

private fun PeerConnection.PeerConnectionState.toPeerChannelState(): PeerChannelConnectionState = when (this) {
    PeerConnection.PeerConnectionState.NEW -> PeerChannelConnectionState.New
    PeerConnection.PeerConnectionState.CONNECTING -> PeerChannelConnectionState.Connecting
    PeerConnection.PeerConnectionState.CONNECTED -> PeerChannelConnectionState.Connected
    PeerConnection.PeerConnectionState.DISCONNECTED -> PeerChannelConnectionState.Disconnected
    PeerConnection.PeerConnectionState.FAILED -> PeerChannelConnectionState.Failed
    PeerConnection.PeerConnectionState.CLOSED -> PeerChannelConnectionState.Closed
}

private fun MutableStateFlow<MediaTracks>.updateLocalVideoTrack(videoTrack: RealVideoTrack) {
    update { it.copy(localVideoTrack = videoTrack) }
}

private fun MutableStateFlow<MediaTracks>.updateRemoteVideoTrack(videoTrack: RealVideoTrack) {
    update { it.copy(remoteVideoTrack = videoTrack) }
}

private fun MutableStateFlow<MediaTracks>.clearRemoteVideoTrack() {
    val old = getAndUpdate { it.copy(remoteVideoTrack = null) }
    old.remoteVideoTrack?.dispose()
}
