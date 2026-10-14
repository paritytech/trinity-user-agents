package io.paritytech.polkadotapp.tools_media_connection_impl

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import io.paritytech.polkadotapp.tools_media_connection_api.domain.GroupPeerConnection
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelFactory
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelSignaling
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.videoProfile
import io.paritytech.polkadotapp.tools_media_connection_impl.connection.AcceptorConnection
import io.paritytech.polkadotapp.tools_media_connection_impl.connection.InitiatorConnection
import io.paritytech.polkadotapp.tools_media_connection_impl.media.DefaultMediaTrackProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.media.SharedMediaTrackProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.ExternalRtcConfigProvider
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

internal class RealPeerChannelFactory @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val webRtcCore: WebRtcCore,
    private val externalRtcConfigProvider: ExternalRtcConfigProvider,
) : PeerChannelFactory {
    override suspend fun createSingleConnection(
        signaling: PeerChannelSignaling,
        mediaConfiguration: MediaConfiguration,
        scope: CoroutineScope,
        isInitiator: Boolean,
        logger: PeerConnectionLogger
    ): PeerChannel {
        val externalRtcConfig = externalRtcConfigProvider.getConfig()
        val mediaTrackProvider = DefaultMediaTrackProvider(
            context,
            webRtcCore,
            mediaConfiguration.videoProfile()
        )

        val channel = if (isInitiator) {
            InitiatorConnection(
                signaling = signaling,
                mediaConfiguration = mediaConfiguration,
                mediaTrackProvider = mediaTrackProvider,
                webRtcCore = webRtcCore,
                externalRtcConfig = externalRtcConfig,
                scope = scope,
                logger = logger
            )
        } else {
            AcceptorConnection(
                signaling = signaling,
                mediaConfiguration = mediaConfiguration,
                mediaTrackProvider = mediaTrackProvider,
                webRtcCore = webRtcCore,
                externalRtcConfig = externalRtcConfig,
                scope = scope,
                logger = logger
            )
        }

        scope.invokeOnCompletion {
            mediaTrackProvider.dispose()
        }

        return channel
    }

    override suspend fun createGroupConnection(
        mediaConfiguration: MediaConfiguration,
        scope: CoroutineScope
    ): GroupPeerConnection {
        val externalRtcConfig = externalRtcConfigProvider.getConfig()
        val mediaTrackProvider = SharedMediaTrackProvider(
            context,
            webRtcCore,
            mediaConfiguration.videoProfile()
        )

        scope.invokeOnCompletion {
            mediaTrackProvider.dispose()
        }

        return RealGroupPeerConnection(
            mediaConfiguration = mediaConfiguration,
            scope = scope,
            webRtcCore = webRtcCore,
            mediaTrackProvider = mediaTrackProvider,
            externalRtcConfig = externalRtcConfig
        )
    }
}
