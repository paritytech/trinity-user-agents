package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class RealVanillaTryRecoverFromBackupAndCreateAccountUseCaseTest {

    private val createNewAccountUseCase: CreateNewAccountUseCase = mock()

    private val useCase = RealVanillaTryRecoverFromBackupAndCreateAccountUseCase(
        createNewAccountUseCase = createNewAccountUseCase,
    )

    @Test
    fun `returns NoNeedToBackup when account creation succeeds`() = runBlocking {
        whenever(createNewAccountUseCase.invoke()).thenReturn(Result.success(Unit))

        val result = useCase()

        assertTrue(result.isSuccess)
        assertEquals(BackupOutcome.NoNeedToBackup, result.getOrNull())
    }

    @Test
    fun `propagates failure from createNewAccount`() = runBlocking {
        val error = IllegalStateException("account creation failed")
        whenever(createNewAccountUseCase.invoke()).thenReturn(Result.failure(error))

        val result = useCase()

        assertEquals(error, result.exceptionOrNull())
    }
}
