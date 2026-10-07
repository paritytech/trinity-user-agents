package io.paritytech.polkadotapp.app.root.presentation.debug

import kotlinx.coroutines.flow.StateFlow

interface DebugMenuContract {
    val state: StateFlow<DebugMenuState>

    fun onBackClick()

    fun onClearBackupClick()

    fun onShareLogsClick()

    fun onCopyWalletAccountClick()

    fun onCopyCandidateAccountClick()

    fun onCopyWalletMnemonicClick()

    fun onOpenVideoGameClick()

    fun onProductBotsClick()

    fun onPocketFacePreviewClick()

    fun onRandomizeAccountClick()

    fun onOpenSpaBrowserClick()

    fun onSpaBrowserUrlEntered(url: String)

    fun onSpaBrowserDialogDismissed()

    fun onClearDotNsCacheClick()

    fun onClearJWTTokenClick()

    fun onSimulateGameResultsClick()

    fun onCoinageDebugWidgetsToggled(enabled: Boolean)

    fun onTruapiRuntimeToggled(enabled: Boolean)

    fun onWasmiWorkerRuntimeToggled(enabled: Boolean)

    fun onRuntimeRestartConfirmed()

    fun onRuntimeRestartCancelled()
}
