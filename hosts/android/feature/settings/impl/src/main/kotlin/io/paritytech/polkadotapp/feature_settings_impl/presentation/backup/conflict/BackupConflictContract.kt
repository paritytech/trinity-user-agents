package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict

import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.models.BackupConflictStep
import kotlinx.coroutines.flow.StateFlow

interface BackupConflictContract {
    val step: StateFlow<BackupConflictStep>

    fun proceedToOverride()
    fun confirmOverride()
    fun cancelOverride()
}
