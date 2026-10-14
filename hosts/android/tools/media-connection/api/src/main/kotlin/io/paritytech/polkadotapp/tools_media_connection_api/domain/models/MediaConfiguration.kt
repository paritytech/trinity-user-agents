package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

sealed interface MediaConfiguration {
    data object None : MediaConfiguration
    data class VideoOnly(
        val initialCameraEnabled: Boolean,
        val videoProfile: VideoEncodingProfile
    ) : MediaConfiguration
    data class AudioVideo(
        val initialCameraEnabled: Boolean,
        val initialMicrophoneEnabled: Boolean,
        val videoProfile: VideoEncodingProfile
    ) : MediaConfiguration
}

fun MediaConfiguration.shouldHaveVideo(): Boolean {
    return this is MediaConfiguration.VideoOnly || this is MediaConfiguration.AudioVideo
}

fun MediaConfiguration.cameraInitiallyEnabled(): Boolean = when (this) {
    MediaConfiguration.None -> false
    is MediaConfiguration.VideoOnly -> initialCameraEnabled
    is MediaConfiguration.AudioVideo -> initialCameraEnabled
}

fun MediaConfiguration.videoProfile(): VideoEncodingProfile? = when (this) {
    is MediaConfiguration.VideoOnly -> videoProfile
    is MediaConfiguration.AudioVideo -> videoProfile
    MediaConfiguration.None -> null
}
