package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models

import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrack
import kotlinx.collections.immutable.ImmutableList
import kotlin.time.Duration

sealed interface CallUiState {
    data object Initializing : CallUiState

    data class Incoming(val initiatedWithVideo: Boolean) : CallUiState

    data class Outgoing(val status: Status) : CallUiState {
        enum class Status { Requesting, Ringing }
    }

    data object Connecting : CallUiState

    data class InProgress(
        val duration: Duration,
        val cameraOn: Boolean,
        val micMuted: Boolean,
        val audioDevices: ImmutableList<AudioDeviceUiModel>,
        val remoteCameraOn: Boolean,
        val remoteMicMuted: Boolean,
        val localVideoTrack: VideoTrack?,
        val remoteVideoTrack: VideoTrack?,
    ) : CallUiState

    sealed interface Terminal : CallUiState

    data object Ended : Terminal

    data object Failed : Terminal
}
