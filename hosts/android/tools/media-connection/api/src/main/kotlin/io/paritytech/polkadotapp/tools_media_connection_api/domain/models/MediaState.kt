package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

data class MediaState(
    val localCameraEnabled: Boolean = false,
    val localMicrophoneEnabled: Boolean = false,
    val remoteCameraEnabled: Boolean = false,
    val remoteMicrophoneEnabled: Boolean = false
)
