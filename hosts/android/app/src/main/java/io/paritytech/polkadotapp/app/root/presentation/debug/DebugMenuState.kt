package io.paritytech.polkadotapp.app.root.presentation.debug

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsOverride

@Immutable
data class DebugMenuState(
    val truapiHostVersion: String,
    val isClearingBackup: Boolean = false,
    val isSharingLogs: Boolean = false,
    val showSpaBrowserDialog: Boolean = false,
    val devServerAvailable: Boolean = false,
    val showDevServerDialog: Boolean = false,
    val hasJWTToken: Boolean = false,
    val coinageDebugWidgetsEnabled: Boolean = true,
    val truapiRuntimeEnabled: Boolean = true,
    val materialsOverride: MaterialsOverride = MaterialsOverride.REMOTE_CONFIG,
    /** Non-null while the restart prompt is up; what to restore if it is declined. */
    val pendingRestart: PendingRestart? = null,
)

/** A setting the app reads only at start, changed and waiting for a restart. */
sealed interface PendingRestart {
    data class Runtime(val revertsTo: Boolean) : PendingRestart

    data class Materials(val revertsTo: MaterialsOverride) : PendingRestart
}
