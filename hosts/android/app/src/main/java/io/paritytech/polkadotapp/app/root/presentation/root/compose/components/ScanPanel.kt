package io.paritytech.polkadotapp.app.root.presentation.root.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.AddContactPanel
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanPanel.compose.EmbeddedQrScanner

@Composable
internal fun ScanPanel(
    modifier: Modifier,
    scannerShape: Shape,
    cameraActive: Boolean,
    onScanHandled: (navigate: (() -> Unit)?) -> Unit,
) {
    AddContactPanel(
        modifier = modifier,
        scannerShape = scannerShape,
        scanner = { scannerModifier, recognitionArmed ->
            EmbeddedQrScanner(
                modifier = scannerModifier,
                cameraActive = cameraActive,
                recognitionArmed = recognitionArmed,
                onScanHandled = onScanHandled,
            )
        },
    )
}
