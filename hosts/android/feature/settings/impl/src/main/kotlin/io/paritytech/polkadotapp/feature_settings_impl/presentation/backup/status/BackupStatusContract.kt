package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status

import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.models.BackupStatusUiState
import kotlinx.coroutines.flow.StateFlow

interface BackupStatusContract {
    val state: StateFlow<BackupStatusUiState>

    fun back()
    fun onShowMnemonic()
    fun onCreateBackup()
    fun onAllowGoogleDrive()
    fun onDeclineGoogleDrive()
    fun onBackupOverriden()
    fun onOverrideBackup()
}
