package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.Timestamp

sealed interface BackupState {
    data object None : BackupState
    data class Available(val createdAt: Timestamp) : BackupState
    data class Conflict(val createdAt: Timestamp) : BackupState
    data object Corrupted : BackupState
    data object NoAccess : BackupState
}
