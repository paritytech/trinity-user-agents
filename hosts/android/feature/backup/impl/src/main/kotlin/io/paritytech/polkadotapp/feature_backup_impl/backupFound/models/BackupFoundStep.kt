package io.paritytech.polkadotapp.feature_backup_impl.backupFound.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.Timestamp

@Immutable
sealed interface BackupFoundStep {
    data class Recover(val backupCreatedAt: Timestamp) : BackupFoundStep
    data object Override : BackupFoundStep
}
