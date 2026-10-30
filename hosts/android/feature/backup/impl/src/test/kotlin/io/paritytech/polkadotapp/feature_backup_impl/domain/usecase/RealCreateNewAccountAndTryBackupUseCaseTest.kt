package io.paritytech.polkadotapp.feature_backup_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import io.paritytech.polkadotapp.tools_backup_api.domain.usecase.CreateAndSaveBackupFromMnemonicUseCase
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RealCreateNewAccountAndTryBackupUseCaseTest {
    private val accountRepository: AccountRepository = mockk(relaxUnitFun = true)
    private val createBackupFromMnemonic: CreateAndSaveBackupFromMnemonicUseCase = mockk()

    private val useCase = RealCreateNewAccountAndTryBackupUseCase(
        accountRepository = accountRepository,
        createBackupFromMnemonic = createBackupFromMnemonic,
    )

    @Test
    fun `returns Created when backup succeeds`() = runBlocking {
        givenBackupResult(Result.success(Unit))

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.Created, result.getOrNull())
        coVerify(exactly = 1) { accountRepository.initAccounts(any()) }
    }

    @Test
    fun `returns AccountsCreatedButBackupFailed when backup fails`() = runBlocking {
        givenBackupResult(Result.failure(RuntimeException("backup unavailable")))

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.AccountsCreatedButBackupFailed, result.getOrNull())
        coVerify(exactly = 1) { accountRepository.initAccounts(any()) }
    }

    private suspend fun givenBackupResult(result: Result<Unit>) {
        coEvery { createBackupFromMnemonic.invoke(any<Mnemonic>()) } returns result
    }
}
