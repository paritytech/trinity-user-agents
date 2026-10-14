package io.paritytech.polkadotapp.tools_media_connection_impl

import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.tools_media_connection_api.domain.GroupPeerConnection
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelSignaling
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrack
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.cameraInitiallyEnabled
import io.paritytech.polkadotapp.tools_media_connection_impl.connection.AcceptorConnection
import io.paritytech.polkadotapp.tools_media_connection_impl.connection.InitiatorConnection
import io.paritytech.polkadotapp.tools_media_connection_impl.media.MediaTrackProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow

internal class RealGroupPeerConnection(
    private val mediaConfiguration: MediaConfiguration,
    private val scope: CoroutineScope,
    private val webRtcCore: WebRtcCore,
    private val mediaTrackProvider: MediaTrackProvider,
    private val externalRtcConfig: ExternalRtcConfig,
) : GroupPeerConnection {
    override val localVideoTrack = MutableStateFlow<VideoTrack?>(null)

    override suspend fun initLocalMedia() {
        ensureSharedLocalMedia()
    }

    override fun createPeer(
        signaling: PeerChannelSignaling,
        isInitiator: Boolean,
        logger: PeerConnectionLogger
    ): PeerChannel {
        val peerScope = scope.childScope(supervised = true)

        return if (isInitiator) {
            InitiatorConnection(
                signaling = signaling,
                mediaConfiguration = mediaConfiguration,
                mediaTrackProvider = mediaTrackProvider,
                webRtcCore = webRtcCore,
                externalRtcConfig = externalRtcConfig,
                scope = peerScope,
                logger = logger
            )
        } else {
            AcceptorConnection(
                signaling = signaling,
                mediaConfiguration = mediaConfiguration,
                mediaTrackProvider = mediaTrackProvider,
                webRtcCore = webRtcCore,
                externalRtcConfig = externalRtcConfig,
                scope = peerScope,
                logger = logger
            )
        }
    }

    override fun disposePeer(peerChannel: PeerChannel) {
        peerChannel.dispose()
    }

    override fun setLocalVideoEnabled(enabled: Boolean) {
        mediaTrackProvider.setVideoEnabled(enabled)
    }

    private suspend fun ensureSharedLocalMedia() {
        when (mediaConfiguration) {
            MediaConfiguration.None -> return
            is MediaConfiguration.AudioVideo,
            is MediaConfiguration.VideoOnly -> {
                if (localVideoTrack.value != null) return

                val rawTrack = mediaTrackProvider.getOrCreateVideoTrack()
                mediaTrackProvider.setVideoEnabled(mediaConfiguration.cameraInitiallyEnabled())
                localVideoTrack.value = RealVideoTrack(rawTrack, webRtcCore.eglBase.eglBaseContext)
                // no need to set up audio track here, we don't need to expose it anywhere now
            }
        }
    }
}
