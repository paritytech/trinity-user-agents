package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetLedger
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.CoinageInstallationRegistrar
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinRecyclingEvaluator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.CoinageHandoffGuard
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.RealCoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Before
import org.junit.Test

/**
 * Clearing uncommitted handoff marks at startup.
 *
 * The mark is the only thing keeping a coin whose key is on its way to a peer from being selected again, and
 * the startup clear deletes every uncommitted one. A ViewModel that is recreated in a live process must
 * therefore not trigger it: the send in flight would lose its reservation and the wallet could re-spend coins
 * the peer already holds the keys to.
 */
class CoinageStartupHandoffReleaseTest {
    private val assetLedger = mockk<CoinageAssetLedger>()

    private val handoffGuard = CoinageHandoffGuard()

    private val transactionService = RealCoinageTransactionService(
        engine = mockk<DurableTransactionService>(relaxed = true),
        assetLedger = assetLedger,
        coinKeypairDerivation = mockk<CoinKeypairDerivation> {
            coEvery { deriveKeypair(any()) } returns mockk { every { publicKey } returns ByteArray(32) }
        },
        voucherRingDerivation = mockk<VoucherRingDerivation>(relaxed = true),
        handoffGuard = handoffGuard,
    )

    private val starter = RealCoinageServiceStarter(
        coinageBackupService = mockk(relaxed = true),
        voucherLocationService = mockk<VoucherLocationService>(relaxed = true),
        coinPresenceSyncService = mockk<CoinPresenceSyncService>(relaxed = true),
        coinRecyclingEvaluator = mockk<CoinRecyclingEvaluator>(relaxed = true),
        observeAccountOnboardingStatusUseCase = mockk<ObserveAccountOnboardingStatusUseCase> {
            // Never completes: an onboarding flow that ends would fail its collector and cancel the scope.
            every { this@mockk.invoke() } returns MutableStateFlow(AccountOnboardingStatus.EMPTY)
        },
        coinageTransactionService = transactionService,
        installationRegistrar = mockk<CoinageInstallationRegistrar>(relaxed = true),
        handoffGuard = handoffGuard,
    )

    private val scope = CoroutineScope(UnconfinedTestDispatcher())

    @Before
    fun setUp() {
        coEvery { assetLedger.markHandedOff(any()) } returns Result.success(Unit)
        coEvery { assetLedger.commitHandoffs(any()) } returns Result.success(Unit)
        coEvery { assetLedger.releaseUncommittedHandoffs() } returns Result.success(Unit)
        coEvery { assetLedger.releaseUncommittedHandoffs(any()) } returns Result.success(Unit)
    }

    @After
    fun stop() = scope.cancel()

    @Test
    fun `the startup release is skipped while a handoff is live`() = runTest {
        reserveHandoff()

        startServices()

        assertStartupReleases(0)
    }

    @Test
    fun `the startup release runs once the live handoff is committed`() = runTest {
        val handoff = reserveHandoff()

        startServices()
        handoff.commit()
        startServices()

        assertStartupReleases(1)
    }

    @Test
    fun `the startup release runs once the live handoff is released`() = runTest {
        val handoff = reserveHandoff()

        startServices()
        handoff.release()
        startServices()

        assertStartupReleases(1)
    }

    @Test
    fun `the startup release runs at most once per process`() = runTest {
        startServices()
        startServices()

        assertStartupReleases(1)
    }

    private suspend fun reserveHandoff(): CoinageHandoffCommit =
        transactionService.preCommitHandoff(listOf(OwnAsset.Coin(testKey(1)))).getOrThrow()

    private fun startServices() = with(ComputationalScope(scope)) { starter.start() }

    private fun assertStartupReleases(times: Int) =
        coVerify(exactly = times) { assetLedger.releaseUncommittedHandoffs() }
}
