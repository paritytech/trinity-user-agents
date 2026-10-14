@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_calls_impl.presentation.call

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_calls_impl.domain.call.CallInteractor
import io.paritytech.polkadotapp.feature_calls_impl.domain.call.CallerDisplay
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioDevicesState
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioRouteController
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.AudioDeviceUiModel
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallUiState
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallerDisplayUiModel
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAvatar
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaTracks
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject
import kotlin.time.Duration

@HiltViewModel
class CallViewModel @Inject constructor(
    private val callStateHolder: CallStateHolder,
    private val callAudioRouteController: CallAudioRouteController,
    private val callController: CallController,
    private val callInteractor: CallInteractor,
) : BaseViewModel() {
    private val activeCallFlow = callStateHolder.observeActiveCall().filterNotNull().shareInBackground()

    private val callerDisplayFlow = activeCallFlow
        .map { it.chatId }
        .distinctUntilChanged()
        .mapLatest { callInteractor.getCallerDisplay(it) }
        .shareInBackground()

    val callerDisplay: StateFlow<CallerDisplayUiModel> = callerDisplayFlow
        .map { it.toUi() }
        .stateIn(this, SharingStarted.Eagerly, CallerDisplayUiModel.Empty)

    val state: StateFlow<CallUiState> = activeCallFlow
        .flatMapLatest(::uiStateFor)
        .stateIn(this, SharingStarted.Eagerly, CallUiState.Initializing)

    fun acceptCall() {
        if (state.value !is CallUiState.Incoming) return
        val activeCall = callStateHolder.getActiveCall() ?: return

        launch {
            val callerName = callerDisplayFlow.first().name

            callController.acceptCall(
                chatId = activeCall.chatId,
                offerId = activeCall.offerId,
                callerName = callerName,
                withVideo = activeCall.initiatedWithVideo,
            )
        }
    }

    fun declineCall() {
        callController.declineCall()
    }

    fun endCall() {
        callController.endCall()
    }

    fun toggleCamera() {
        val inProgress = state.value as? CallUiState.InProgress ?: return
        callController.setCameraEnabled(!inProgress.cameraOn)
    }

    fun toggleMicrophone() {
        val inProgress = state.value as? CallUiState.InProgress ?: return
        callController.setMicrophoneEnabled(inProgress.micMuted)
    }

    fun selectAudioDevice(deviceId: Int) {
        callController.selectAudioDevice(deviceId)
    }

    private fun uiStateFor(call: ActiveCallState): Flow<CallUiState> {
        val status = call.status

        return when {
            status is CallStatus.Failed -> flowOf(CallUiState.Failed)
            status is CallStatus.Ended -> flowOf(CallUiState.Ended)

            status is CallStatus.Connected -> combine(
                callStateHolder.observeMediaTracks(),
                callStateHolder.observeMediaState(),
                callAudioRouteController.audioDevices,
            ) { tracks, media, audio -> inProgressOf(status.duration, tracks, media, audio) }

            call.direction == CallDirection.Incoming && status is CallStatus.Ringing ->
                flowOf(CallUiState.Incoming(initiatedWithVideo = call.initiatedWithVideo))

            call.direction == CallDirection.Outgoing && status is CallStatus.Ringing ->
                flowOf(CallUiState.Outgoing(CallUiState.Outgoing.Status.Ringing))

            status is CallStatus.Requesting ->
                flowOf(CallUiState.Outgoing(CallUiState.Outgoing.Status.Requesting))

            status is CallStatus.Connecting -> flowOf(CallUiState.Connecting)

            else -> flowOf(CallUiState.Initializing)
        }
    }

    private fun inProgressOf(
        duration: Duration,
        tracks: MediaTracks,
        media: MediaState,
        audio: CallAudioDevicesState,
    ) = CallUiState.InProgress(
        duration = duration,
        cameraOn = media.localCameraEnabled,
        micMuted = !media.localMicrophoneEnabled,
        audioDevices = audio.toUi(),
        remoteCameraOn = media.remoteCameraEnabled,
        remoteMicMuted = !media.remoteMicrophoneEnabled,
        localVideoTrack = tracks.localVideoTrack,
        remoteVideoTrack = tracks.remoteVideoTrack,
    )
}

private fun CallAudioDevicesState.toUi(): ImmutableList<AudioDeviceUiModel> =
    devices
        .map { AudioDeviceUiModel(it.id, it.type, it.name, isSelected = it.id == selectedId) }
        .toImmutableList()

private fun CallerDisplay.toUi() = CallerDisplayUiModel(
    name = name,
    avatar = avatar?.toUi(),
)

private fun ContactAvatar.toUi(): AvatarUiModel = when (this) {
    is ContactAvatar.Account -> AvatarUiModel.Name(name, AvatarColorScheme.from(themeSeed))
    is ContactAvatar.Url -> AvatarUiModel.Image(url)
}
