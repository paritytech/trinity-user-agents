package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.CreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class RealGPTryRecoverFromBackupAndCreateAccountUseCaseTest {

    private val accountRepository: AccountRepository = mock()
    private val createNewAccountUseCase: CreateNewAccountUseCase = mock()
    private val backupService: BackupService = mock()
    private val createNewAccountAndTryBackupUseCase: CreateNewAccountAndTryBackupUseCase = mock()

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
        whenever(accountRepository.deriveWalletAccountId(entropy)).thenReturn(accountId)

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.ExistingBackupFound(createdAt, accountId), result.getOrNull())
    }

    @Test
    fun `delegates to createNewAccountAndTryBackup when no restorable backup found`() = runBlocking {
        givenServiceReturns(Result.success(null))
        whenever(createNewAccountAndTryBackupUseCase.invoke()).thenReturn(Result.success(BackupOutcome.Created))

        val result = useCase()

        assertEquals(BackupOutcome.Created, result.getOrNull())
    }

    @Test
    fun `falls back to createNewAccount when service fails`() = runBlocking {
        givenServiceReturns(Result.failure(IllegalStateException("service down")))
        whenever(createNewAccountUseCase.invoke()).thenReturn(Result.success(Unit))

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
    }

    @Test
    fun `falls back to createNewAccount when restore fails`() = runBlocking {
        val restorable = givenRestorableBackup(createdAt, Result.failure(RuntimeException("restore failed")))
        givenServiceReturns(Result.success(restorable))
        whenever(createNewAccountUseCase.invoke()).thenReturn(Result.success(Unit))

        val result = useCase()

        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
    }

    @Test
    fun `propagates failure when fallback createNewAccount fails`() = runBlocking {
        givenServiceReturns(Result.failure(IllegalStateException("service down")))
        val fallbackError = IllegalStateException("account creation failed")
        whenever(createNewAccountUseCase.invoke()).thenReturn(Result.failure(fallbackError))

        val result = useCase()

        assertEquals(fallbackError, result.exceptionOrNull())
    }

    @Test
    fun `falls back to createNewAccount when downstream createNewAccountAndTryBackup fails`() = runBlocking {
        givenServiceReturns(Result.success(null))
        whenever(createNewAccountAndTryBackupUseCase.invoke()).thenReturn(Result.failure(IllegalStateException("downstream failure")))
        whenever(createNewAccountUseCase.invoke()).thenReturn(Result.success(Unit))

        val result = useCase()

        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
    }

    private suspend fun givenServiceReturns(result: Result<RestorableBackup?>) {
        whenever(backupService.getRestorableBackup()).thenReturn(result)
    }

    private fun givenRestorableBackup(createdAt: Long, restoreResult: Result<Backup>): RestorableBackup {
        val restorable: RestorableBackup = mock()
        whenever(restorable.createdAt).thenReturn(createdAt)
        runBlocking { whenever(restorable.restore()).thenReturn(restoreResult) }
        return restorable
    }
}
