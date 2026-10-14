package io.paritytech.polkadotapp.tools_media_connection_api.domain

import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaTracks
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.PeerChannelConnectionState
import kotlinx.coroutines.flow.StateFlow

interface PeerChannel {
    val connectionState: StateFlow<PeerChannelConnectionState>
    val dataTransport: DataTransport
    val mediaTracks: StateFlow<MediaTracks>
    val mediaState: StateFlow<MediaState>

    fun startConnection()
    fun dispose()

    suspend fun setLocalCameraEnabled(enabled: Boolean)
    suspend fun setLocalMicrophoneEnabled(enabled: Boolean)
}
