package io.paritytech.polkadotapp.feature_backup_api.domain.usecase

interface ImportAccountFromBackupUseCase {
    suspend operator fun invoke(): Result<Unit>
}
