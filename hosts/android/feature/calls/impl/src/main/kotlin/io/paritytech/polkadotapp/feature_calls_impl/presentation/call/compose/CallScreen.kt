package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.KeepScreenOn
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.CallViewModel
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components.CallAudioDeviceSheet
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components.CallControlsRow
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components.CallerInfo
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components.LocalVideoPreview
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components.MicOffWarnings
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallUiState
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallerDisplayUiModel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrack
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.delay
import kotlin.time.Duration.Companion.seconds

private val CALL_ENDED_LINGER = 2.seconds

@Composable
fun CallScreen(
    viewModel: CallViewModel,
    closeCallScreen: () -> Unit,
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val callerDisplay by viewModel.callerDisplay.collectAsStateWithLifecycle()

    (state as? CallUiState.InProgress)?.let { state ->
        if (state.cameraOn || state.remoteCameraOn) {
            KeepScreenOn()
        }
    }

    LaunchedEffect(state is CallUiState.Terminal) {
        if (state is CallUiState.Terminal) {
            delay(CALL_ENDED_LINGER)
            closeCallScreen()
        }
    }

    val context = LocalContext.current
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { results ->
        if (results.values.all { it }) viewModel.acceptCall() else viewModel.declineCall()
    }

    val onAccept = {
        val incoming = state as? CallUiState.Incoming
        if (incoming != null) {
            val required = buildList {
                add(Manifest.permission.RECORD_AUDIO)
                if (incoming.initiatedWithVideo) add(Manifest.permission.CAMERA)
            }
            val allGranted = required.all { permission ->
                ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
            }
            if (allGranted) {
                viewModel.acceptCall()
            } else {
                permissionLauncher.launch(required.toTypedArray())
            }
        }
    }

    val cameraPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (granted) viewModel.toggleCamera()
    }

    val onToggleCamera = {
        val inProgress = state as? CallUiState.InProgress
        if (inProgress != null) {
            val cameraGranted = ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.CAMERA
            ) == PackageManager.PERMISSION_GRANTED
            if (inProgress.cameraOn || cameraGranted) {
                viewModel.toggleCamera()
            } else {
                cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
            }
        }
    }

    var audioSheetVisible by remember { mutableStateOf(false) }

    CallScreenContent(
        onBack = closeCallScreen,
        state = state,
        callerDisplay = callerDisplay,
        onAccept = onAccept,
        onAudioClick = { audioSheetVisible = true },
        onToggleCamera = onToggleCamera,
        onToggleMicrophone = viewModel::toggleMicrophone,
        onEndCall = viewModel::endCall,
    )

    CallAudioDeviceSheet(
        isVisible = audioSheetVisible,
        devices = (state as? CallUiState.InProgress)?.audioDevices ?: persistentListOf(),
        onSelect = { deviceId ->
            viewModel.selectAudioDevice(deviceId)
            audioSheetVisible = false
        },
        onDismiss = { audioSheetVisible = false },
    )
}

@Composable
private fun CallScreenContent(
    onBack: () -> Unit,
    state: CallUiState,
    callerDisplay: CallerDisplayUiModel,
    onAccept: () -> Unit,
    onAudioClick: () -> Unit,
    onToggleCamera: () -> Unit,
    onToggleMicrophone: () -> Unit,
    onEndCall: () -> Unit,
) {
    val inProgress = state as? CallUiState.InProgress
    val remoteVideo: VideoTrack? = inProgress
        ?.takeIf { it.remoteCameraOn }
        ?.remoteVideoTrack
    val localVideo: VideoTrack? = inProgress
        ?.takeIf { it.cameraOn }
        ?.localVideoTrack

    PolkadotSurface {
        Box(modifier = Modifier.fillMaxSize()) {
            remoteVideo?.Render(
                modifier = Modifier.fillMaxSize(),
                isMirrored = false,
            )

            Column(modifier = Modifier.fillMaxSize()) {
                CallTopSection(
                    onBack = onBack,
                    state = state.takeIf { remoteVideo != null },
                    callerDisplay = callerDisplay,
                )

                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .weight(1f),
                ) {
                    if (remoteVideo == null) {
                        Column(
                            modifier = Modifier
                                .fillMaxSize()
                                .padding(56.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.Center,
                        ) {
                            CallerInfo(
                                state = state,
                                callerDisplay = callerDisplay,
                                showAvatar = true,
                            )
                        }
                    }

                    if (localVideo != null) {
                        LocalVideoPreview(
                            modifier = Modifier
                                .align(Alignment.BottomEnd)
                                .padding(PolkadotTheme.spacings.small)
                                .size(width = 96.dp, height = 144.dp),
                            videoTrack = localVideo,
                        )
                    }

                    if (inProgress != null) {
                        MicOffWarnings(
                            modifier = Modifier
                                .align(Alignment.BottomCenter)
                                .padding(PolkadotTheme.spacings.mediumIncreased),
                            selfMicMuted = inProgress.micMuted,
                            remoteMicMuted = inProgress.remoteMicMuted,
                            remoteCallerName = callerDisplay.name,
                        )
                    }
                }

                CallControlsRow(
                    state = state,
                    onAccept = onAccept,
                    onAudioClick = onAudioClick,
                    onToggleCamera = onToggleCamera,
                    onToggleMicrophone = onToggleMicrophone,
                    onEndCall = onEndCall,
                )
            }
        }
    }
}

@Composable
private fun CallTopSection(
    onBack: () -> Unit,
    state: CallUiState?,
    callerDisplay: CallerDisplayUiModel,
) {
    Column {
        PolkadotTopBar(
            modifier = Modifier.fillMaxWidth(),
            navigationAction = rememberTopBarAction(action = onBack),
            titleAlignment = TopBarTitleAlignment.Center,
        )

        if (state != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 56.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                CallerInfo(state = state, callerDisplay = callerDisplay)
            }
        }
    }
}
