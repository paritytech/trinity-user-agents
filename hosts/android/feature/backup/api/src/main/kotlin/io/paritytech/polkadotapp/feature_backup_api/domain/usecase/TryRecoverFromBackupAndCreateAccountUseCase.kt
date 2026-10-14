package io.paritytech.polkadotapp.feature_backup_api.domain.usecase

import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome

interface TryRecoverFromBackupAndCreateAccountUseCase {
    suspend operator fun invoke(): Result<BackupOutcome>
}
