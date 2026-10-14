package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

data class VideoEncodingProfile(
    val captureTargetHeight: Int,
    val captureFps: Int,
    val maxBitrateBps: Int,
    val degradationPreference: VideoDegradationPreference
) {
    companion object {
        const val UNLIMITED_BITRATE_BPS = 0
    }
}

enum class VideoDegradationPreference {
    MAINTAIN_FRAMERATE,
    MAINTAIN_RESOLUTION,
    BALANCED,
    DISABLED
}
