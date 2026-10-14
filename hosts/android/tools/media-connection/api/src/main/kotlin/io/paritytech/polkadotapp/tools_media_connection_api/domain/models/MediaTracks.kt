package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

data class MediaTracks(
    val localVideoTrack: VideoTrack? = null,
    val remoteVideoTrack: VideoTrack? = null
)
