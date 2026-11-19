package io.paritytech.polkadotapp.feature_backup_impl.backupFound.domain

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.CreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.ImportAccountFromBackupUseCase
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import javax.inject.Inject

interface BackupFoundInteractor {
    suspend fun importAccountFromBackup(): Result<Unit>
    suspend fun createAccountsAndOverrideBackup(): Result<Unit>
    suspend fun getUsername(accountId: AccountId): Result<String?>
}

class RealBackupFoundInteractor @Inject constructor(
    private val importAccountFromBackupUseCase: ImportAccountFromBackupUseCase,
    private val createNewAccountAndTryBackupUseCase: CreateNewAccountAndTryBackupUseCase,
    private val resourcesRepository: ResourcesRepository,
    private val knownChains: KnownChains,
) : BackupFoundInteractor {
    override suspend fun importAccountFromBackup(): Result<Unit> {
        return importAccountFromBackupUseCase()
    }

    override suspend fun createAccountsAndOverrideBackup(): Result<Unit> {
        return createNewAccountAndTryBackupUseCase().coerceToUnit()
    }

    override suspend fun getUsername(accountId: AccountId): Result<String?> {
        return resourcesRepository.consumerInfo(knownChains.assetHub, accountId)
            .map { consumerInfo -> consumerInfo?.username }
    }
}
