package io.paritytech.polkadotapp.tools_media_connection_api.domain

import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import kotlinx.coroutines.CoroutineScope

interface PeerChannelFactory {
    suspend fun createSingleConnection(
        signaling: PeerChannelSignaling,
        mediaConfiguration: MediaConfiguration,
        scope: CoroutineScope,
        isInitiator: Boolean,
        logger: PeerConnectionLogger
    ): PeerChannel

    suspend fun createGroupConnection(
        mediaConfiguration: MediaConfiguration,
        scope: CoroutineScope
    ): GroupPeerConnection
}
