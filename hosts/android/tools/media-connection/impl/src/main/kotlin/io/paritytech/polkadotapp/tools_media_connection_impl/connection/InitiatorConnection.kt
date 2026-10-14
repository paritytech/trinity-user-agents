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
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.addCandidates
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.createOffer
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.setLocalDescription
import io.paritytech.polkadotapp.tools_media_connection_impl.utils.setRemoteDescription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import org.webrtc.DataChannel
import org.webrtc.SessionDescription

internal class InitiatorConnection(
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
    init {
        initializeDataChannel()
    }

    override fun startConnection() = launchUnit {
        logger.log("Starting connection")
        val offerSdp = connection.createOffer()
        logger.log("Offer created")

        connection.setLocalDescription(offerSdp)
        logger.log("Local description set")

        val initialCandidates = awaitInitialCandidates()
        logger.log("Initial candidates gathered: count=${initialCandidates.size}")

        val offerSetup = SdpCoderSetup(
            setupSdp = offerSdp.description,
            candidates = initialCandidates
        )
        val offerSdpEncoded = sdpCoder.encodeSetup(offerSetup)

        signaling.sendOffer(offerSdpEncoded)
        logger.log("Offer sent")

        val encodedAnswer = signaling.awaitIncomingAnswer()
        logger.log("Answer received")

        val answerSetup = sdpCoder.decodeSetup(encodedAnswer)
        val answerSdp = SessionDescription(
            SessionDescription.Type.ANSWER,
            answerSetup.setupSdp
        )

        connection.setRemoteDescription(answerSdp)
        logger.log("Remote description set")

        connection.addCandidates(answerSetup.candidates)
        logger.log("Added ${answerSetup.candidates.size} candidates from answer")

        startSendingLocalIceCandidates()

        if (mediaConfiguration != MediaConfiguration.None) {
            performMediaRenegotiation()
        }

        logger.log("Connection established")
    }

    private suspend fun performMediaRenegotiation() {
        initiateLocalMediaTracks()

        val mediaOfferSdp = connection.createOffer()
        connection.setLocalDescription(mediaOfferSdp)
        applyVideoSenderParams()

        sendSignal(PeerConnectionSignal.Offer(mediaOfferSdp.description))
        logger.log("Sent media offer via data channel")

        val mediaAnswerSdp = awaitMultimediaAnswer()
        connection.setRemoteDescription(mediaAnswerSdp)
        logger.log("Received media answer via data channel and set as remote description")
        logNegotiatedVideoCodec(mediaAnswerSdp)
    }

    private suspend fun awaitMultimediaAnswer(): SessionDescription {
        return subscribeSignals()
            .filterIsInstance<PeerConnectionSignal.Answer>()
            .map { SessionDescription(SessionDescription.Type.ANSWER, it.sdp) }
            .first()
    }

    private fun initializeDataChannel() {
        val dataChannel = connection.createDataChannel("dataChannel", DataChannel.Init())
        dataChannelMessaging.init(dataChannel)
    }
}
