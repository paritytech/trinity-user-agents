package io.paritytech.polkadotapp.app.root.presentation.debug

import androidx.compose.runtime.Immutable

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
    /** Non-null while the restart prompt is up; the value to restore if it is declined. */
    val runtimeRestartRevertsTo: Boolean? = null,
    val productChatEnabled: Boolean = false,
    /** Non-null while the restart prompt for the chat mode is up; the value to restore if it is declined. */
    val chatModeRestartRevertsTo: Boolean? = null,
)
