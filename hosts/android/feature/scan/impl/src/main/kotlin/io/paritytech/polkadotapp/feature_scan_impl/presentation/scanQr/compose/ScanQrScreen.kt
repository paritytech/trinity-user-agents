package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr.compose

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanQr.ScanQrViewModel
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanning.compose.QrScannerScreen

@Composable
fun ScanQrScreen(viewModel: ScanQrViewModel) {
    QrScannerScreen(
        surfaceRequestFlow = viewModel.surfaceRequest,
        invalidCodeEvent = viewModel.invalidCodeEvent,
        cameraPermissionDeniedFlow = viewModel.cameraPermissionDenied,
        bindToCamera = viewModel::bindToCamera,
        onInvalidCodeAlertClosed = viewModel::invalidationDialogClosed,
        onPermissionAlertClosed = viewModel::permissionAlertClosed,
        onCloseClick = viewModel::back
    )
}
