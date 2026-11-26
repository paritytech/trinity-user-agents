package io.paritytech.polkadotapp.app.root.presentation.debug

import androidx.compose.runtime.Immutable

@Immutable
data class DebugMenuState(
    val isClearingBackup: Boolean = false,
    val isSharingLogs: Boolean = false,
    val showSpaBrowserDialog: Boolean = false,
    val devServerAvailable: Boolean = false,
    val showDevServerDialog: Boolean = false,
    val hasJWTToken: Boolean = false,
    val coinageDebugWidgetsEnabled: Boolean = true,
)
