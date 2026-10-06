package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_products_impl.domain.deriveEntropy.RealDeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.StoredUsername
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.HostRejection

class TrUAPILocalSessionSourceTest {
    private val accountRepository = mockk<AccountRepository>()
    private val accountSecretsStorage = mockk<AccountSecretsStorage>()
    private val usernameOfAccountUseCase = mockk<UsernameOfAccountUseCase>()
    private val source = TrUAPILocalSessionSource(accountRepository, usernameOfAccountUseCase)
    private val secrets = TrUAPIWalletSecretProvider(accountRepository, accountSecretsStorage)
    private val metaId = 1L
    private val walletEntropy = ByteArray(16) { 0xAB.toByte() }

    @Before
    fun setUp() {
        coEvery { accountRepository.getWalletAccount() } returns wallet(metaId)
        coEvery { accountSecretsStorage.getMetaAccountPassphrase(metaId) } returns MnemonicCreator.fromEntropy(walletEntropy)
        coEvery { usernameOfAccountUseCase.getUsername() } returns Result.success(null)
    }

    @Test
    fun `the provider returns raw wallet entropy rather than product derivation`() = runTest {
        val derived = RealDeriveEntropyUseCase(accountRepository, accountSecretsStorage)
            .deriveRootEntropySource().getOrThrow()
        val entropy = secrets.readWalletRootEntropy(metaId.toString())

        assertArrayEquals(walletEntropy, entropy)
        assertTrue(!derived.contentEquals(entropy))
    }

    @Test
    fun `public activation metadata does not read the root secret`() = runTest {
        coEvery { usernameOfAccountUseCase.getUsername() } returns Result.success(
            StoredUsername(
                fullUsername = null,
                liteUsername = Username.fromParts("alice", index = 7),
                isOnChain = false,
            ),
        )

        assertEquals(TrUAPILocalSession("1", "alice.07"), source.resolve(metaId).getOrThrow())
        coVerify(exactly = 0) { accountSecretsStorage.getMetaAccountPassphrase(any()) }
    }

    @Test
    fun `a failed username read still yields public activation metadata`() = runTest {
        coEvery { usernameOfAccountUseCase.getUsername() } returns Result.failure(IllegalStateException())

        assertEquals(TrUAPILocalSession("1", null), source.resolve(metaId).getOrThrow())
    }

    @Test
    fun `a missing passphrase is a declared provider failure`() = runTest {
        coEvery { accountSecretsStorage.getMetaAccountPassphrase(metaId) } returns null

        assertTrue(runCatching { secrets.readWalletRootEntropy("1") }.exceptionOrNull() is HostRejection)
    }

    @Test
    fun `another or noncanonical wallet id never reads protected storage`() = runTest {
        for (id in listOf("2", "01", "+1", "wallet")) {
            assertTrue(runCatching { secrets.readWalletRootEntropy(id) }.exceptionOrNull() is HostRejection)
        }
        coVerify(exactly = 0) { accountSecretsStorage.getMetaAccountPassphrase(any()) }
    }

    @Test
    fun `selection changing during a protected read rejects the captured root`() = runTest {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        coEvery { accountSecretsStorage.getMetaAccountPassphrase(metaId) } coAnswers {
            entered.complete(Unit)
            release.await()
            MnemonicCreator.fromEntropy(walletEntropy)
        }
        val pending = async { runCatching { secrets.readWalletRootEntropy("1") } }
        entered.await()
        coEvery { accountRepository.getWalletAccount() } returns wallet(2)
        release.complete(Unit)

        assertTrue(pending.await().exceptionOrNull() is HostRejection)
        coVerify(exactly = 1) { accountSecretsStorage.getMetaAccountPassphrase(1) }
        coVerify(exactly = 0) { accountSecretsStorage.getMetaAccountPassphrase(2) }
    }

    @Test
    fun `username from a superseding wallet is not attached to the captured activation`() = runTest {
        coEvery { usernameOfAccountUseCase.getUsername() } coAnswers {
            coEvery { accountRepository.getWalletAccount() } returns wallet(2)
            Result.success(null)
        }

        assertTrue(source.resolve(metaId).isFailure)
    }

    private fun wallet(id: Long): MetaAccount = mockk {
        every { this@mockk.id } returns id
        every { purpose } returns MetaAccount.Purpose.WALLET
    }
}
