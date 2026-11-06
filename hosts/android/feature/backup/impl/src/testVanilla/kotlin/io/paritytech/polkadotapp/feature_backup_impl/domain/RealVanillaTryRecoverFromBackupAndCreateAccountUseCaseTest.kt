package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RealVanillaTryRecoverFromBackupAndCreateAccountUseCaseTest {

    private val createNewAccountUseCase: CreateNewAccountUseCase = mockk()

    private val useCase = RealVanillaTryRecoverFromBackupAndCreateAccountUseCase(
        createNewAccountUseCase = createNewAccountUseCase,
    )

    @Test
    fun `returns NoNeedToBackup when account creation succeeds`() = runBlocking {
        coEvery { createNewAccountUseCase.invoke() } returns Result.success(Unit)

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.NoNeedToBackup, result.getOrNull())
    }

    @Test
    fun `propagates failure from createNewAccount`() = runBlocking {
        val error = IllegalStateException("account creation failed")
        coEvery { createNewAccountUseCase.invoke() } returns Result.failure(error)

        val result = useCase()

        assertEquals(error, result.exceptionOrNull())
    }
}
