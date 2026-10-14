package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.Timestamp

@Immutable
sealed interface BackupConflictStep {
    data class Conflict(val backupCreatedAt: Timestamp) : BackupConflictStep
    data class Override(val inProgress: Boolean = false) : BackupConflictStep
}
