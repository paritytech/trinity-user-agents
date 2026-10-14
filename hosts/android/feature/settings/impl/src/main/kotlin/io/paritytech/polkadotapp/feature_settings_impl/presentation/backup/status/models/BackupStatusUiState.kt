package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.models

import io.paritytech.polkadotapp.common.domain.model.Timestamp

sealed interface BackupStatusUiState {
    data object CheckingForBackup : BackupStatusUiState
    data object NoAccess : BackupStatusUiState
    data object NoBackup : BackupStatusUiState
    data object BackupInProgress : BackupStatusUiState
    data object BackupExists : BackupStatusUiState
    data class BackupConflict(val createdAt: Timestamp) : BackupStatusUiState
}
