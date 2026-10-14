package io.paritytech.polkadotapp.feature_backup_impl.backupFound

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundProgressState
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundStep
import kotlinx.coroutines.flow.StateFlow

interface BackupFoundContract {
    val progressState: StateFlow<BackupFoundProgressState>
    val step: StateFlow<BackupFoundStep>
    val username: StateFlow<LoadingState<String?>>

    fun backupOverrideIntention()
    fun backupOverrideConfirm()
    fun backupOverrideCancel()

    fun recoverBackup()
}
