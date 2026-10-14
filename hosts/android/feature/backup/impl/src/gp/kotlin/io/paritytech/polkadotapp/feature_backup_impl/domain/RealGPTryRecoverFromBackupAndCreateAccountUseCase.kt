package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.CreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.TryRecoverFromBackupAndCreateAccountUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup
import javax.inject.Inject

class RealGPTryRecoverFromBackupAndCreateAccountUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val createNewAccountUseCase: CreateNewAccountUseCase,
    private val backupService: BackupService,
    private val createNewAccountAndTryBackupUseCase: CreateNewAccountAndTryBackupUseCase
) : TryRecoverFromBackupAndCreateAccountUseCase {

    override suspend fun invoke(): Result<BackupOutcome> {
        return backupService.getRestorableBackup()
            .flatMap {
                it?.deriveExistingBackupOutcome() ?: createNewAccountAndTryBackupUseCase()
            }
            .flatRecover {
                createNewAccountUseCase()
                    .map { BackupOutcome.AccountsCreatedButBackupFailed }
            }
    }

    private suspend fun RestorableBackup.deriveExistingBackupOutcome() = restore().map { backup ->
        val accountId = accountRepository.deriveWalletAccountId(backup.value)
        BackupOutcome.ExistingBackupFound(createdAt, accountId)
    }
}
