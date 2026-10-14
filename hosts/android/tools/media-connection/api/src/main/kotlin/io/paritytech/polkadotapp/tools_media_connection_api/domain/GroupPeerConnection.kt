package io.paritytech.polkadotapp.tools_media_connection_api.domain

import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrack
import kotlinx.coroutines.flow.StateFlow

interface GroupPeerConnection {
    val localVideoTrack: StateFlow<VideoTrack?>

    suspend fun initLocalMedia()

    fun createPeer(
        signaling: PeerChannelSignaling,
        isInitiator: Boolean,
        logger: PeerConnectionLogger
    ): PeerChannel

    fun disposePeer(peerChannel: PeerChannel)

    /**
     * Toggles local video capture without tearing down the connection.
     * `enabled = false` releases the camera HW and disables the track — the
     * remote peer sees the video go muted while the WebRTC session stays
     * alive. Safe to call repeatedly with the same value.
     */
    fun setLocalVideoEnabled(enabled: Boolean)
}
