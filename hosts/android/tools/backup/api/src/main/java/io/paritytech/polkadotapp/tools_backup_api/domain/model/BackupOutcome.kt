package io.paritytech.polkadotapp.tools_backup_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp

sealed interface BackupOutcome {
    data object Created : BackupOutcome

    data class ExistingBackupFound(
        val createdAt: Timestamp,
        val accountId: AccountId
    ) : BackupOutcome

    data object AccountsCreatedButBackupFailed : BackupOutcome

    data object NoNeedToBackup : BackupOutcome
}
