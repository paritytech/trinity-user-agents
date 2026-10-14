package io.paritytech.polkadotapp.tools_media_connection_impl.media

import android.content.Context
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoEncodingProfile
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import org.webrtc.AudioTrack
import org.webrtc.VideoTrack

internal class SharedMediaTrackProvider(
    context: Context,
    webRtcCore: WebRtcCore,
    videoProfile: VideoEncodingProfile?
) : MediaTrackProvider {
    private val default = DefaultMediaTrackProvider(context, webRtcCore, videoProfile)

    override suspend fun getOrCreateVideoTrack(): VideoTrack = default.getOrCreateVideoTrack()

    override suspend fun getOrCreateAudioTrack(): AudioTrack = default.getOrCreateAudioTrack()

    override fun setVideoEnabled(enabled: Boolean) = default.setVideoEnabled(enabled)

    override fun setAudioEnabled(enabled: Boolean) = default.setAudioEnabled(enabled)

    override fun dispose() {
        default.dispose()
    }
}
