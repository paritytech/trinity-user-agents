package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanning.compose

import androidx.camera.compose.CameraXViewfinder
import androidx.camera.core.SurfaceRequest
import androidx.camera.viewfinder.core.ImplementationMode
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.utils.openAppSettings
import io.paritytech.polkadotapp.design.components.dialog.NovaAlertDialog
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import io.paritytech.polkadotapp.common.R as RCommon

internal class PermissionMissingHint(
    val missing: StateFlow<Boolean>,
    val content: @Composable (onRetry: () -> Unit) -> Unit,
)

@Composable
internal fun QrViewfinder(
    modifier: Modifier = Modifier,
    surfaceRequestFlow: StateFlow<SurfaceRequest?>,
    invalidCodeEvent: SharedFlow<Unit>,
    cameraPermissionDeniedFlow: StateFlow<Boolean>,
    cameraActive: Boolean,
    bindToCamera: suspend (LifecycleOwner) -> Unit,
    onInvalidCodeAlertClosed: () -> Unit,
    onPermissionAlertClosed: () -> Unit,
    implementationMode: ImplementationMode,
    permissionMissingHint: PermissionMissingHint?,
) {
    val owner = LocalLifecycleOwner.current
    var bindAttempt by remember { mutableIntStateOf(0) }
    LaunchedEffect(owner, bindAttempt, cameraActive) {
        if (cameraActive) bindToCamera(owner)
    }

    Box(modifier = modifier) {
        val surfaceRequest by surfaceRequestFlow.collectAsStateWithLifecycle()
        surfaceRequest?.let {
            CameraXViewfinder(
                modifier = Modifier.fillMaxSize(),
                surfaceRequest = it,
                implementationMode = implementationMode,
            )
        }

        if (permissionMissingHint != null) {
            val cameraPermissionMissing by permissionMissingHint.missing.collectAsStateWithLifecycle()
            if (cameraPermissionMissing) {
                permissionMissingHint.content { bindAttempt++ }
            }
        }
    }

    var invalidQrAlertIsVisible by remember { mutableStateOf(false) }
    invalidCodeEvent.collectAsEffect { _, _ ->
        invalidQrAlertIsVisible = true
    }

    if (invalidQrAlertIsVisible) {
        NovaAlertDialog(
            title = stringResource(RCommon.string.scan_invalid_code_error_title),
            text = stringResource(RCommon.string.scan_invalid_code_error_message),
            positiveButtonTitle = stringResource(RCommon.string.scan_invalid_code_error_action),
            onPositiveButtonClick = {
                invalidQrAlertIsVisible = false
                onInvalidCodeAlertClosed()
            },
            onDismissRequest = {
                invalidQrAlertIsVisible = false
                onInvalidCodeAlertClosed()
            }
        )
    }

    val cameraPermissionDenied by cameraPermissionDeniedFlow.collectAsStateWithLifecycle()
    if (cameraPermissionDenied) {
        val context = LocalContext.current

        NovaAlertDialog(
            title = stringResource(RCommon.string.common_permission_permissions_denied_title),
            text = stringResource(RCommon.string.common_permission_permissions_denied_message),
            positiveButtonTitle = stringResource(RCommon.string.common_to_settings),
            onPositiveButtonClick = {
                onPermissionAlertClosed()
                context.openAppSettings()
            },
            negativeButtonTitle = stringResource(RCommon.string.common_cancel),
            onNegativeButtonClick = onPermissionAlertClosed,
            onDismissRequest = onPermissionAlertClosed
        )
    }
}
