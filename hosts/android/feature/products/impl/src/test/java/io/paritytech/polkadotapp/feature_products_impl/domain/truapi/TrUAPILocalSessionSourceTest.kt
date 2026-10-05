package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class TrUAPILocalSessionSourceTest {
    private val accounts = mockk<AccountRepository>()
    private val secrets = mockk<AccountSecretsStorage>()
    private val usernames = mockk<UsernameOfAccountUseCase>()
    private val keyguard = mockk<android.app.KeyguardManager> { every { isDeviceLocked } returns false }
    private val context = mockk<android.content.Context> {
        every { getSystemService(android.app.KeyguardManager::class.java) } returns keyguard
    }
    private val source = TrUAPILocalSessionSource(accounts, secrets, usernames, context)

    init {
        coEvery { accounts.getWalletAccount() } returns mockk { every { id } returns 1L }
        coEvery { usernames.getUsername() } returns Result.success(null)
    }

    @Test
    fun `runtime configuration selects a wallet without reading its root`() = runTest {
        val session = source.resolve().getOrThrow()
        assertEquals("1" to null, session.walletId to session.liteUsername)
        coVerify(exactly = 0) { secrets.getMetaAccountPassphrase(any()) }
    }

    @Test
    fun `wallet provider reads the protected root only for the selected owner`() = runTest {
        val entropy = ByteArray(16) { 0xab.toByte() }
        coEvery { secrets.getMetaAccountPassphrase(1L) } returns MnemonicCreator.fromEntropy(entropy)
        assertArrayEquals(entropy, source.readWalletRootEntropy("1"))
        val failure = runCatching { source.readWalletRootEntropy("2") }
        assertEquals("Selected wallet changed before activation", failure.exceptionOrNull()?.message)
        coVerify(exactly = 1) { secrets.getMetaAccountPassphrase(1L) }
    }

    @Test
    fun `device lock before activation cannot read protected root material`() = runTest {
        every { keyguard.isDeviceLocked } returns true
        val error = runCatching { source.readWalletRootEntropy("1") }.exceptionOrNull()
        assertEquals("Unlock the device before activating the wallet", error?.message)
        coVerify(exactly = 0) { secrets.getMetaAccountPassphrase(any()) }
    }

    @Test
    fun `account switch during protected storage read fences activation`() = runTest {
        coEvery { accounts.getWalletAccount() } returnsMany listOf(mockk { every { id } returns 1L }, mockk { every { id } returns 2L })
        coEvery { secrets.getMetaAccountPassphrase(1L) } returns MnemonicCreator.fromEntropy(ByteArray(16))
        val error = runCatching { source.readWalletRootEntropy("1") }.exceptionOrNull()
        assertEquals("Wallet activation was invalidated", error?.message)
    }

    @Test
    fun `missing protected root fails activation`() = runTest {
        coEvery { secrets.getMetaAccountPassphrase(1L) } returns null
        org.junit.Assert.assertTrue(runCatching { source.readWalletRootEntropy("1") }.isFailure)
    }
}
