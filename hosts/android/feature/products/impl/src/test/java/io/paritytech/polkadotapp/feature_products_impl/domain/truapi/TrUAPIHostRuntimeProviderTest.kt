package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.truapi.AuthState
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionRecord
import uniffi.truapi.SecretCoreStorageKey
import java.util.concurrent.ConcurrentHashMap

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIHostRuntimeProviderTest {
    @get:Rule
    val directory = TemporaryFolder()
    private val selected = MutableStateFlow(wallet(1))
    private val accounts = mockk<AccountRepository> {
        coEvery { getWalletAccount() } answers { selected.value }
        every { walletAccountFlow() } returns selected
    }
    private val secrets = mockk<AccountSecretsStorage> {
        coEvery { getMetaAccountPassphrase(any()) } returns MnemonicCreator.fromEntropy(ByteArray(32) { 1 })
    }

    private val protectedValues = ConcurrentHashMap<SecretCoreStorageKey, ByteArray>()
    private val secretStorage = mockk<TrUAPISecretStorage> {
        coEvery { read(any()) } answers { protectedValues[firstArg()] }
        coEvery { write(any(), any()) } answers { protectedValues[firstArg()] = secondArg() }
        coEvery { clear(any()) } answers { protectedValues.remove(firstArg()); Unit }
    }

    @Test
    fun `saved settings share the constructed runtime while wallet activation is unavailable`() = runTest {
        coEvery { secrets.getMetaAccountPassphrase(1) } returns null
        val provider = provider(StandardTestDispatcher(testScheduler))
        provider.constructedRuntime().getOrThrow().use { runtime ->
            assertTrue(runCatching { runtime.statementRenewalOwnerKey() }.isFailure)
            val observedInitial = CompletableDeferred<Unit>()
            val snapshots = async {
                runtime.observePermissionRecords("calendar.dot")
                    .onEach { observedInitial.complete(Unit) }
                    .take(2)
                    .toList()
            }
            observedInitial.await()
            val request = PermissionAuthorizationRequest.IdentityDisclosure
            runtime.setPermissionRecord("calendar.dot", request, PermissionAuthorizationStatus.DENIED)
            assertEquals(
                listOf(emptyList(), listOf(PermissionRecord("calendar.dot", request, PermissionAuthorizationStatus.DENIED))),
                snapshots.await()
            )
            coVerify(exactly = 0) { secrets.getMetaAccountPassphrase(any()) }

            assertTrue(provider.runtime().isFailure)
            assertSame(runtime, provider.constructedRuntime().getOrThrow())
            coVerify(exactly = 1) { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) }
        }
    }

    @Test
    fun `cancelled startup and changed selection share the pending installation key creation`() = runTest {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        coEvery { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) } coAnswers {
            entered.complete(Unit)
            release.await()
            protectedValues[SecretCoreStorageKey.StorageEncryptionKey] = secondArg()
        }
        val provider = provider(StandardTestDispatcher(testScheduler))
        val first = async { provider.runtime() }
        entered.await()
        first.cancelAndJoin()
        selected.value = wallet(2)
        val second = async { provider.runtime().getOrThrow() }
        runCurrent()
        assertFalse(second.isCompleted)
        coVerify(exactly = 0) { secrets.getMetaAccountPassphrase(any()) }
        release.complete(Unit)

        second.await().use { runtime ->
            assertSame(runtime, provider.runtime().getOrThrow())
            assertTrue(runtime.statementRenewalOwnerKey().isNotEmpty())
            coVerify(exactly = 1) { secretStorage.read(SecretCoreStorageKey.StorageEncryptionKey) }
            coVerify(exactly = 1) { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) }
            coVerify(exactly = 1) { secrets.getMetaAccountPassphrase(2) }
            coVerify(exactly = 0) { secrets.getMetaAccountPassphrase(1) }
        }
    }

    @Test
    fun `failed installation key persistence fails construction before wallet activation and can retry`() = runTest {
        val failure = IllegalStateException("Protected storage unavailable")
        coEvery { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) } throws failure
        val provider = provider(StandardTestDispatcher(testScheduler))
        assertTrue(provider.runtime().isFailure)
        coVerify(exactly = 0) { secrets.getMetaAccountPassphrase(any()) }

        coEvery { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) } answers {
            protectedValues[SecretCoreStorageKey.StorageEncryptionKey] = secondArg()
        }
        provider.runtime().getOrThrow().use { runtime ->
            assertTrue(runtime.statementRenewalOwnerKey().isNotEmpty())
            coVerify(exactly = 2) { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) }
        }
    }

    @Test
    fun `closing a startup waiter does not cancel shared activation`() = runTest {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        coEvery { secrets.getMetaAccountPassphrase(1) } coAnswers {
            entered.complete(Unit)
            release.await()
            MnemonicCreator.fromEntropy(ByteArray(32) { 1 })
        }
        val provider = provider(StandardTestDispatcher(testScheduler))
        val first = async { provider.runtime() }
        entered.await()
        first.cancelAndJoin()
        val second = async { provider.runtime().getOrThrow() }
        release.complete(Unit)
        val runtime = second.await()
        try {
            assertSame(runtime, provider.runtime().getOrThrow())
            assertTrue(runtime.statementRenewalOwnerKey().isNotEmpty())
            coVerify(exactly = 1) { secrets.getMetaAccountPassphrase(1) }
        } finally {
            runtime.close()
        }
    }

    @Test
    fun `protected root failure keeps the constructed runtime for the next activation attempt`() = runTest {
        coEvery { secrets.getMetaAccountPassphrase(1) } returns null
        val provider = provider(StandardTestDispatcher(testScheduler))
        assertTrue(provider.runtime().isFailure)
        coEvery { secrets.getMetaAccountPassphrase(1) } returns MnemonicCreator.fromEntropy(ByteArray(32) { 1 })

        provider.runtime().getOrThrow().use { runtime ->
            assertTrue(runtime.statementRenewalOwnerKey().isNotEmpty())
            coVerify(exactly = 1) { secretStorage.read(SecretCoreStorageKey.StorageEncryptionKey) }
            coVerify(exactly = 1) { secretStorage.write(SecretCoreStorageKey.StorageEncryptionKey, any()) }
        }
    }

    @Test
    fun `wallet selection locks the old wallet before a suspended replacement and keeps it locked on failure`() = runTest {
        val provider = provider(StandardTestDispatcher(testScheduler))
        val runtime = provider.runtime().getOrThrow()
        try {
            runCurrent()
            val originalOwner = runtime.statementRenewalOwnerKey()
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            coEvery { secrets.getMetaAccountPassphrase(2) } coAnswers {
                entered.complete(Unit)
                release.await()
                null
            }
            selected.value = wallet(2)
            entered.await()
            assertTrue(runCatching { runtime.statementRenewalOwnerKey() }.isFailure)
            release.complete(Unit)
            runCurrent()
            assertTrue(runCatching { runtime.statementRenewalOwnerKey() }.isFailure)

            coEvery { secrets.getMetaAccountPassphrase(3) } returns MnemonicCreator.fromEntropy(ByteArray(32) { 3 })
            selected.value = wallet(3)
            provider.sessionState.first { it is AuthState.Connected }
            assertFalse(originalOwner.contentEquals(runtime.statementRenewalOwnerKey()))
        } finally {
            runtime.close()
        }
    }

    private fun provider(dispatcher: kotlinx.coroutines.CoroutineDispatcher): TrUAPIHostRuntimeProvider {
        val context = mockk<Context> { every { noBackupFilesDir } returns directory.root }
        val chain = mockk<Chain> { every { genesisHash } returns DataByteArray(ByteArray(32) { 1 }) }
        val chains = mockk<ChainRegistry> { coEvery { getChain(any()) } returns chain }
        val usernames = mockk<UsernameOfAccountUseCase> { coEvery { getUsername() } returns Result.success(null) }
        val tld = mockk<DotNsTldProvider> { coEvery { getTld() } returns Result.success(DotNsTld.parse("dot")!!) }
        val lifecycle = mockk<AppLifecycleObserver> { every { subscribe() } returns emptyFlow() }
        val contacts = mockk<AppContactsHostBridge> { every { contactRemovals() } returns emptyFlow() }
        val dispatchers = mockk<CoroutineDispatchers> { every { computation } returns dispatcher }
        val chainDirectory = mockk<TrUAPIChainDirectory> { coEvery { resolve() } returns EMPTY_CHAINS }
        return TrUAPIHostRuntimeProvider(
            context,
            chains,
            KnownChains("people", "asset", "bulletin", null),
            chainDirectory,
            TrUAPILocalSessionSource(accounts, usernames),
            TrUAPIWalletSecretProvider(accounts, secrets),
            accounts,
            tld,
            secretStorage,
            OkHttpClient(),
            mockk(),
            lifecycle,
            contacts,
            mockk(),
            dispatchers,
        )
    }

    private fun wallet(id: Long): MetaAccount = mockk {
        every { this@mockk.id } returns id
        every { purpose } returns MetaAccount.Purpose.WALLET
    }
}
