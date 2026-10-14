package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_products_impl.domain.deriveEntropy.RealDeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class TrUAPILocalSessionSourceTest {
    private val accountRepository: AccountRepository = mockk()
    private val accountSecretsStorage: AccountSecretsStorage = mockk()
    private val localUsernameStorage: LocalUsernameStorage = mockk()
    private val source = TrUAPILocalSessionSource(
        accountRepository,
        accountSecretsStorage,
        localUsernameStorage,
    )

    private val metaId = 1L
    private val walletEntropy = ByteArray(16) { 0xAB.toByte() }

    @Before
    fun setUp() {
        runBlocking {
            val metaAccount: MetaAccount = mockk()
            every { metaAccount.id } returns metaId
            coEvery { accountRepository.getWalletAccount() } returns metaAccount
            coEvery { accountSecretsStorage.getMetaAccountPassphrase(metaId) } returns
                MnemonicCreator.fromEntropy(walletEntropy)
            coEvery { localUsernameStorage.getValue() } returns null
        }
    }

    /**
     * The core derives the session's root and identity keypairs from this value,
     * so anything derived here would give the same recovery phrase different
     * product accounts than iOS derives from it.
     */
    @Test
    fun `the secret is the wallet's raw entropy`() = runBlocking {
        val session = source.resolve().getOrNull()

        assertArrayEquals(walletEntropy, session?.secret)
    }

    @Test
    fun `the secret is not the product entropy derivation`() = runBlocking {
        val derived = RealDeriveEntropyUseCase(accountRepository, accountSecretsStorage)
            .deriveRootEntropySource()
            .getOrNull()

        val session = source.resolve().getOrNull()

        assertTrue(derived != null && !derived.contentEquals(session?.secret))
    }

    @Test
    fun `the lite username is reported when there is one`() = runBlocking {
        coEvery { localUsernameStorage.getValue() } returns Username.fromParts("alice", index = 7)

        assertEquals("alice.07", source.resolve().getOrNull()?.liteUsername)
    }

    @Test
    fun `a missing passphrase fails rather than yielding an empty secret`() = runBlocking {
        coEvery { accountSecretsStorage.getMetaAccountPassphrase(metaId) } returns null

        assertTrue(source.resolve().isFailure)
    }

    @Test
    fun `no stored username still yields a session`() = runBlocking {
        val session = source.resolve().getOrThrow()

        assertNull(session.liteUsername)
        assertTrue(session.secret.isNotEmpty())
    }
}
