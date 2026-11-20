package io.paritytech.polkadotapp.app.debug.e2e

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.confirmVerified
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_backup_impl.ManualMnemonicInteractor
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test

class SeedAccountsTest {
    private val interactor: ManualMnemonicInteractor = mockk {
        coEvery { restoreAccountWithEnteredMnemonic(any()) } returns Result.success(Unit)
        coEvery { createAccounts(any()) } returns Result.success(Unit)
    }

    @Test
    fun `restores the person of the phrase so the app signs in as that person`() = runTest {
        // Given
        val mnemonic = "legal winner thank year wave sausage worth useful legal winner thank yellow"

        // When
        val result = seedAccounts(interactor, accountsInitialized = false, mnemonic = mnemonic)

        // Then
        assertTrue(result.isSuccess)
        coVerify(exactly = 1) { interactor.restoreAccountWithEnteredMnemonic(mnemonic) }
        confirmVerified(interactor)
    }

    @Test
    fun `creates the fixed e2e identity when no phrase is given`() = runTest {
        // Given
        val fixedEntropy = ByteArray(16) { 0x42 }

        // When
        val result = seedAccounts(interactor, accountsInitialized = false, mnemonic = null)

        // Then
        assertTrue(result.isSuccess)
        coVerify(exactly = 1) { interactor.createAccounts(match { it.contentEquals(fixedEntropy) }) }
        confirmVerified(interactor)
    }

    @Test
    fun `keeps the accounts that exist so a second broadcast does not replace the person`() = runTest {
        // Given
        val mnemonic = "legal winner thank year wave sausage worth useful legal winner thank yellow"

        // When
        val result = seedAccounts(interactor, accountsInitialized = true, mnemonic = mnemonic)

        // Then
        assertTrue(result.isSuccess)
        confirmVerified(interactor)
    }
}
