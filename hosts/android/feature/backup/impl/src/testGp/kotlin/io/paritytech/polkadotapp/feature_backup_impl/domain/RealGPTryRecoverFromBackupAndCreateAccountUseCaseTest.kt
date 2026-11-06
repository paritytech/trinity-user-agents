package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.CreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RealGPTryRecoverFromBackupAndCreateAccountUseCaseTest {

    private val accountRepository: AccountRepository = mockk()
    private val createNewAccountUseCase: CreateNewAccountUseCase = mockk()
    private val backupService: BackupService = mockk()
    private val createNewAccountAndTryBackupUseCase: CreateNewAccountAndTryBackupUseCase = mockk()

    private val useCase = RealGPTryRecoverFromBackupAndCreateAccountUseCase(
        accountRepository = accountRepository,
        createNewAccountUseCase = createNewAccountUseCase,
        backupService = backupService,
        createNewAccountAndTryBackupUseCase = createNewAccountAndTryBackupUseCase,
    )

    private val entropy = ByteArray(32) { it.toByte() }
    private val accountId: AccountId = entropy.toDataByteArray()
    private val createdAt = 12_345L

    @Test
    fun `returns ExistingBackupFound when restorable backup restores successfully`() = runBlocking {
        val restorable = givenRestorableBackup(createdAt, Result.success(Backup(entropy)))
        givenServiceReturns(Result.success(restorable))
        coEvery { accountRepository.deriveWalletAccountId(entropy) } returns accountId

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.ExistingBackupFound(createdAt, accountId), result.getOrNull())
    }

    @Test
    fun `delegates to createNewAccountAndTryBackup when no restorable backup found`() = runBlocking {
        givenServiceReturns(Result.success(null))
        coEvery { createNewAccountAndTryBackupUseCase.invoke() } returns Result.success(BackupOutcome.Created)

        val result = useCase()

        assertEquals(BackupOutcome.Created, result.getOrNull())
    }

    @Test
    fun `falls back to createNewAccount when service fails`() = runBlocking {
        givenServiceReturns(Result.failure(IllegalStateException("service down")))
        coEvery { createNewAccountUseCase.invoke() } returns Result.success(Unit)

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
    }

    @Test
    fun `falls back to createNewAccount when restore fails`() = runBlocking {
        val restorable = givenRestorableBackup(createdAt, Result.failure(RuntimeException("restore failed")))
        givenServiceReturns(Result.success(restorable))
        coEvery { createNewAccountUseCase.invoke() } returns Result.success(Unit)

        val result = useCase()

        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
    }

    @Test
    fun `propagates failure when fallback createNewAccount fails`() = runBlocking {
        givenServiceReturns(Result.failure(IllegalStateException("service down")))
        val fallbackError = IllegalStateException("account creation failed")
        coEvery { createNewAccountUseCase.invoke() } returns Result.failure(fallbackError)

        val result = useCase()

        assertEquals(fallbackError, result.exceptionOrNull())
    }

    @Test
    fun `falls back to createNewAccount when downstream createNewAccountAndTryBackup fails`() = runBlocking {
        givenServiceReturns(Result.success(null))
        coEvery { createNewAccountAndTryBackupUseCase.invoke() } returns
            Result.failure(IllegalStateException("downstream failure"))
        coEvery { createNewAccountUseCase.invoke() } returns Result.success(Unit)

        val result = useCase()

        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
    }

    private suspend fun givenServiceReturns(result: Result<RestorableBackup?>) {
        coEvery { backupService.getRestorableBackup() } returns result
    }

    private fun givenRestorableBackup(createdAt: Long, restoreResult: Result<Backup>): RestorableBackup {
        val restorable: RestorableBackup = mockk()
        every { restorable.createdAt } returns createdAt
        coEvery { restorable.restore() } returns restoreResult
        return restorable
    }
}
