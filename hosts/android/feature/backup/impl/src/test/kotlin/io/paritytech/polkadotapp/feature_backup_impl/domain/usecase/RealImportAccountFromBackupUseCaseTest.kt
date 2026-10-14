package io.paritytech.polkadotapp.feature_backup_impl.domain.usecase

import io.paritytech.polkadotapp.common.domain.errors.BackupDecryptionKeyNotFoundException
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_backup_api.domain.error.ImportFromBackupError
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.error.CorruptedBackupException
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify

class RealImportAccountFromBackupUseCaseTest {
    private val backupService: BackupService = mock()
    private val accountRepository: AccountRepository = mock()
    private val backupExistsStorage: BackupExistsStorage = mock()

    private val useCase = RealImportAccountFromBackupUseCase(
        backupService = backupService,
        accountRepository = accountRepository,
        backupExistsStorage = backupExistsStorage,
    )

    private val entropy = ByteArray(32) { it.toByte() }

    @Test
    fun `imports account and marks backup as existing when restorable backup is available`() = runBlocking {
        val restorable = givenRestorableBackup(restoreResult = Result.success(Backup(entropy)))
        givenServiceReturns(Result.success(restorable))

        val result = useCase()

        assertTrue(result.isSuccess)
        verify(accountRepository).initAccounts(entropy)
        verify(backupExistsStorage).setBackupExists(true)
    }

    @Test
    fun `returns NotFound when restorable backup is null`() = runBlocking {
        givenServiceReturns(Result.success<RestorableBackup?>(null))

        val result = useCase()

        assertFalse(result.isSuccess)
        assertEquals(ImportFromBackupError.NotFound, result.exceptionOrNull())
    }

    @Test
    fun `maps BackupDecryptionKeyNotFoundException from service to NotFound`() = runBlocking {
        givenServiceReturns(Result.failure(BackupDecryptionKeyNotFoundException()))

        val result = useCase()

        assertEquals(ImportFromBackupError.NotFound, result.exceptionOrNull())
    }

    @Test
    fun `maps CorruptedBackupException thrown during restore to Corrupted error`() = runBlocking {
        val restorable = givenRestorableBackup(restoreResult = Result.failure(CorruptedBackupException()))
        givenServiceReturns(Result.success(restorable))

        val result = useCase()

        assertEquals(ImportFromBackupError.Corrupted, result.exceptionOrNull())
    }

    @Test
    fun `maps unknown restore failure to Unknown error`() = runBlocking {
        val original = IllegalStateException("boom")
        val restorable = givenRestorableBackup(restoreResult = Result.failure(original))
        givenServiceReturns(Result.success(restorable))

        val result = useCase()

        val error = result.exceptionOrNull()
        assertTrue(error is ImportFromBackupError.Unknown)
        assertEquals(original, (error as ImportFromBackupError.Unknown).original)
    }

    @Test
    fun `passes through ImportFromBackupError thrown during restore`() = runBlocking {
        val restorable = givenRestorableBackup(restoreResult = Result.failure(ImportFromBackupError.Cancelled))
        givenServiceReturns(Result.success(restorable))

        val result = useCase()

        assertEquals(ImportFromBackupError.Cancelled, result.exceptionOrNull())
    }

    private suspend fun givenServiceReturns(result: Result<RestorableBackup?>) {
        whenever(backupService.getRestorableBackup()).thenReturn(result)
    }

    private fun givenRestorableBackup(restoreResult: Result<Backup>): RestorableBackup {
        val restorable: RestorableBackup = mock()
        runBlocking { whenever(restorable.restore()).thenReturn(restoreResult) }
        return restorable
    }
}
