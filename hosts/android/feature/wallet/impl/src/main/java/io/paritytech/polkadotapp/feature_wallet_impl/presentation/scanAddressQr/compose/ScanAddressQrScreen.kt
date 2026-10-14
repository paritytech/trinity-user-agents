package io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr.compose

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.compose.CameraXViewfinder
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.dialog.NovaAlertDialog
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr.ScanAddressQrContract
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ScanAddressQrScreen(contract: ScanAddressQrContract) {
    var permissionGranted by remember { mutableStateOf(false) }
    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        permissionGranted = it
    }

    LaunchedEffect(Unit) {
        permissionLauncher.launch(Manifest.permission.CAMERA)
    }

    val owner = LocalLifecycleOwner.current
    LaunchedEffect(permissionGranted, owner) {
        if (permissionGranted) {
            contract.bindToCamera(owner)
        }
    }

    Box(
        modifier = Modifier.fillMaxSize()
    ) {
        val surfaceRequest by contract.surfaceRequest.collectAsStateWithLifecycle()
        surfaceRequest?.let {
            CameraXViewfinder(
                modifier = Modifier.fillMaxSize(),
                surfaceRequest = it
            )
        }

        Image(
            modifier = Modifier.fillMaxSize(),
            imageVector = ImageOverlay,
            contentScale = ContentScale.Crop,
            contentDescription = "image_overlay"
        )

        PolkadotTopBar(
            modifier = Modifier
                .fillMaxWidth(),
            navigationAction = rememberTopBarAction(contract::back, NovaIcons.Close),
            titleAlignment = TopBarTitleAlignment.Center,
        )
    }

    var invalidQrAlertIsVisible by remember { mutableStateOf(false) }
    contract.invalidAddressEvent.collectAsEffect { _, _ ->
        invalidQrAlertIsVisible = true
    }

    if (invalidQrAlertIsVisible) {
        NovaAlertDialog(
            title = stringResource(RCommon.string.add_withdrawal_scan_invalid_code_error_title),
            text = stringResource(RCommon.string.add_withdrawal_scan_invalid_code_error_message),
            positiveButtonTitle = stringResource(RCommon.string.add_withdrawal_scan_invalid_code_error_action),
            onPositiveButtonClick = { invalidQrAlertIsVisible = false },
            onDismissRequest = { invalidQrAlertIsVisible = false }
        )
    }
}
