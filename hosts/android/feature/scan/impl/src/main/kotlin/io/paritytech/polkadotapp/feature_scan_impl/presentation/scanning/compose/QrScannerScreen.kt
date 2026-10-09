package io.paritytech.polkadotapp.feature_scan_impl.presentation.scanning.compose

import androidx.camera.core.SurfaceRequest
import androidx.camera.viewfinder.core.ImplementationMode
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.LifecycleOwner
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_scan_impl.R
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import io.paritytech.polkadotapp.common.R as RCommon

private val LabelOffsetFromCenter = 250.dp

@Composable
internal fun QrScannerScreen(
    surfaceRequestFlow: StateFlow<SurfaceRequest?>,
    invalidCodeEvent: SharedFlow<Unit>,
    cameraPermissionDeniedFlow: StateFlow<Boolean>,
    bindToCamera: suspend (LifecycleOwner) -> Unit,
    onInvalidCodeAlertClosed: () -> Unit,
    onPermissionAlertClosed: () -> Unit,
    onCloseClick: () -> Unit
) {
    Box(
        modifier = Modifier.fillMaxSize()
    ) {
        QrViewfinder(
            modifier = Modifier.fillMaxSize(),
            surfaceRequestFlow = surfaceRequestFlow,
            invalidCodeEvent = invalidCodeEvent,
            cameraPermissionDeniedFlow = cameraPermissionDeniedFlow,
            cameraActive = true,
            bindToCamera = bindToCamera,
            onInvalidCodeAlertClosed = onInvalidCodeAlertClosed,
            onPermissionAlertClosed = onPermissionAlertClosed,
            implementationMode = ImplementationMode.EXTERNAL,
            permissionMissingHint = null,
        )

        Image(
            modifier = Modifier.fillMaxSize(),
            painter = painterResource(R.drawable.img_scanner_frame),
            contentScale = ContentScale.Crop,
            contentDescription = "image_overlay"
        )

        NovaText(
            modifier = Modifier
                .align(Alignment.Center)
                .offset(y = LabelOffsetFromCenter)
                .padding(horizontal = PolkadotTheme.spacings.extraLargeIncreased),
            text = stringResource(RCommon.string.scan_point_camera_at_qr_code),
            color = PolkadotTheme.colors.fg.staticWhite,
            style = PolkadotTheme.typography.title.medium,
            textAlign = TextAlign.Center
        )

        IconButton(
            modifier = Modifier
                .systemBarsPadding()
                .padding(
                    horizontal = PolkadotTheme.spacings.small,
                    vertical = PolkadotTheme.spacings.smallIncreased
                ),
            onClick = onCloseClick
        ) {
            NovaIcon(
                imageVector = NovaIcons.Close,
                tint = PolkadotTheme.colors.fg.staticWhite
            )
        }
    }
}
