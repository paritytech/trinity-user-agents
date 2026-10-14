package io.paritytech.polkadotapp.feature_backup_impl.recover

import kotlinx.coroutines.flow.StateFlow

interface RecoverOptionsContract {
    val isRecovering: StateFlow<Boolean>

    fun onRecoverFromBackup()

    fun onImportRecoveryPhrase()

    fun onDismiss()
}
