package io.paritytech.polkadotapp.app.root.presentation.root.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.AddContactPanel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.AddContactStep
import io.paritytech.polkadotapp.feature_scan_impl.presentation.scanPanel.compose.EmbeddedQrScanner

@Composable
internal fun ScanPanel(
    modifier: Modifier,
    step: AddContactStep,
    cameraActive: Boolean,
    keyboardRequested: Boolean,
    onSearchFieldClick: () -> Unit,
    onScanClick: () -> Unit,
    onClose: () -> Unit,
    onScanHandled: (navigate: (() -> Unit)?) -> Unit,
) {
    AddContactPanel(
        modifier = modifier,
        step = step,
        keyboardRequested = keyboardRequested,
        onSearchFieldClick = onSearchFieldClick,
        onScanClick = onScanClick,
        onClose = onClose,
        scanner = { scannerModifier ->
            EmbeddedQrScanner(
                modifier = scannerModifier,
                cameraActive = cameraActive,
                recognitionArmed = cameraActive,
                onScanHandled = onScanHandled,
            )
        },
    )
}
