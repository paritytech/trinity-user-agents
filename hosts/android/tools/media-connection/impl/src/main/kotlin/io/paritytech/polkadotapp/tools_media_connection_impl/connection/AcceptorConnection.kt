package io.paritytech.polkadotapp.tools_media_connection_impl.connection

import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelSignaling
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import io.paritytech.polkadotapp.tools_media_connection_impl.media.MediaTrackProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.PeerConnectionSignal
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.SdpCoderSetup
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.SimplePeerConnectionObserver
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.addCandidates
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.createAnswer
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.setLocalDescription
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.setRemoteDescription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import org.webrtc.DataChannel
import org.webrtc.SessionDescription

internal class AcceptorConnection(
    signaling: PeerChannelSignaling,
    mediaConfiguration: MediaConfiguration,
    mediaTrackProvider: MediaTrackProvider,
    webRtcCore: WebRtcCore,
    externalRtcConfig: ExternalRtcConfig,
    scope: CoroutineScope,
    logger: PeerConnectionLogger,
) : PeerChannelConnection(
    signaling = signaling,
    mediaConfiguration = mediaConfiguration,
    mediaTrackProvider = mediaTrackProvider,
    webRtcCore = webRtcCore,
    externalRtcConfig = externalRtcConfig,
    scope = scope,
    logger = logger
) {
    private val acceptorConnectionObserver = object : SimplePeerConnectionObserver() {
        override fun onDataChannel(channel: DataChannel) {
            dataChannelMessaging.init(channel)
        }
    }

    init {
        addObserver(acceptorConnectionObserver)
    }

    override fun startConnection() = launchUnit {
        logger.log("Starting acceptor connection")
        val offerSdpEncoded = signaling.awaitIncomingOffer()
        logger.log("Received encoded offer")

        val offerSetup = sdpCoder.decodeSetup(offerSdpEncoded)
        val offerSdp = SessionDescription(SessionDescription.Type.OFFER, offerSetup.setupSdp)
        connection.setRemoteDescription(offerSdp)
        logger.log("Remote description set (Offer)")

        connection.addCandidates(offerSetup.candidates)
        logger.log("Added ${offerSetup.candidates.size} candidates from offer")

        val answerSdp = connection.createAnswer()
        logger.log("Answer created")

        connection.setLocalDescription(answerSdp)
        logger.log("Local description set (Answer)")

        val initialCandidates = awaitInitialCandidates()
        logger.log("Initial candidates gathered: count=${initialCandidates.size}")

        val answerSetup = SdpCoderSetup(
            setupSdp = answerSdp.description,
            candidates = initialCandidates
        )
        val answerSdpEncoded = sdpCoder.encodeSetup(answerSetup)

        signaling.sendAnswer(answerSdpEncoded)
        logger.log("Answer sent")

        startSendingLocalIceCandidates()

        if (mediaConfiguration != MediaConfiguration.None) {
            performMediaRenegotiation()
        }

        logger.log("Connection established")
    }

    private suspend fun performMediaRenegotiation() {
        val mediaOffer = awaitMultimediaOffer()
        logger.log("Received media offer via data channel")
        connection.setRemoteDescription(mediaOffer)

        initiateLocalMediaTracks()

        val mediaAnswer = connection.createAnswer()
        connection.setLocalDescription(mediaAnswer)
        applyVideoSenderParams()
        logNegotiatedVideoCodec(mediaAnswer)
        logger.log("Created and set local description for media answer")

        sendSignal(PeerConnectionSignal.Answer(mediaAnswer.description))
        logger.log("Sent media answer via data channel")
    }

    private suspend fun awaitMultimediaOffer(): SessionDescription {
        return subscribeSignals()
            .filterIsInstance<PeerConnectionSignal.Offer>()
            .map { SessionDescription(SessionDescription.Type.OFFER, it.sdp) }
            .first()
    }
}
