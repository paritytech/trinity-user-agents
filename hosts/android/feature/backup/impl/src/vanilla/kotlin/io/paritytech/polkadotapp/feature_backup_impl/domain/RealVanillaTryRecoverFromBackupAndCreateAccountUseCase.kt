package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.TryRecoverFromBackupAndCreateAccountUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import javax.inject.Inject

class RealVanillaTryRecoverFromBackupAndCreateAccountUseCase @Inject constructor(
    private val createNewAccountUseCase: CreateNewAccountUseCase
) : TryRecoverFromBackupAndCreateAccountUseCase {

    override suspend fun invoke(): Result<BackupOutcome> {
        return createNewAccountUseCase()
            .map { BackupOutcome.NoNeedToBackup }
    }
}
