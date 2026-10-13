package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanPanel.compose

import androidx.camera.viewfinder.core.ImplementationMode
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanPanel.ScanPanelViewModel
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanning.compose.PermissionMissingHint
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanning.compose.QrViewfinder
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun EmbeddedQrScanner(
    modifier: Modifier = Modifier,
    cameraActive: Boolean,
    recognitionArmed: Boolean,
    onScanHandled: (navigate: (() -> Unit)?) -> Unit,
) {
    val viewModel = hiltViewModel<ScanPanelViewModel>()

    viewModel.scanHandled.collectAsEffect { _, navigate ->
        onScanHandled(navigate)
    }

    LaunchedEffect(recognitionArmed) {
        viewModel.setRecognitionArmed(recognitionArmed)
    }

    QrViewfinder(
        modifier = modifier,
        surfaceRequestFlow = viewModel.surfaceRequest,
        invalidCodeEvent = viewModel.invalidCodeEvent,
        cameraPermissionDeniedFlow = viewModel.cameraPermissionDenied,
        cameraActive = cameraActive,
        bindToCamera = viewModel::bindToCamera,
        onInvalidCodeAlertClosed = viewModel::invalidationDialogClosed,
        onPermissionAlertClosed = viewModel::permissionAlertClosed,
        implementationMode = ImplementationMode.EMBEDDED,
        permissionMissingHint = PermissionMissingHint(missing = viewModel.cameraPermissionMissing) { onRetry ->
            CameraPermissionHint(onRetry = onRetry)
        },
    )
}

@Composable
private fun CameraPermissionHint(onRetry: () -> Unit) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .clickable(onClick = onRetry)
            .padding(PolkadotTheme.spacings.medium),
        contentAlignment = Alignment.Center,
    ) {
        NovaText(
            text = stringResource(RCommon.string.scan_panel_camera_permission_hint),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )
    }
}
