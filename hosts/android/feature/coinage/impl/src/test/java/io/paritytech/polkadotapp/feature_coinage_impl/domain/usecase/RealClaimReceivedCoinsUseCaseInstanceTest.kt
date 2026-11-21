package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.ConfiguredInstanceCoins
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.RecordingConnectionRefCounter
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class RealClaimReceivedCoinsUseCaseInstanceTest {
    @Before
    fun mockDerivation() = mockkStatic(DERIVATION_FILE)

    @After
    fun unmockDerivation() = unmockkStatic(DERIVATION_FILE)

    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinRepository: CoinRepository = mockk()
    private val transactionService: CoinageTransactionService = mockk()
    private val assetValueUseCase: CoinageAssetValueUseCase = mockk()
    private val submissionUseCase: CoinageTransferSubmissionUseCase = mockk()
    private val timeProvider: TimeProvider = mockk()
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider = mockk()

    private val useCase = RealClaimReceivedCoinsUseCase(
        chainAssetProvider = chainAssetProvider,
        coinRepository = coinRepository,
        transactionService = transactionService,
        assetValueUseCase = assetValueUseCase,
        submissionUseCase = submissionUseCase,
        timeProvider = timeProvider,
        configuredInstanceCoins = ConfiguredInstanceCoins(coinageInstanceIdProvider),
        chainConnectionRefCounter = RecordingConnectionRefCounter(),
    )

    private val groupId = CoinageOperationGroupId("group")

    @Before
    fun setUp() {
        every { chainAssetProvider.chainId() } returns "test-chain"
        every { timeProvider.now() } returns WINDOW_OPEN
        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns flow {
            emit(emptyList())
            awaitCancellation()
        }
        coEvery { submissionUseCase(any(), any(), any(), any()) } returns Result.success(Unit)
        coEvery { assetValueUseCase.valueOf(any()) } answers {
            Result.success(BigInteger.valueOf(firstArg<List<OwnAsset>>().size.toLong()).intoBalance())
        }
    }

    @Test
    fun `a coin of the configured instance is claimed when that instance is not 0`() = runTest {
        val coin = key(1)
        givenConfiguredInstance(7u)
        givenChainSees(mapOf(coin.accountId to 7))

        reportsOf(coin)

        assertClaimedOnce(coin)
    }

    @Test
    fun `a payment made only of coins of another instance ends as not claimed`() = runTest {
        val foreign = key(1)
        givenConfiguredInstance(0u)
        givenChainSees(mapOf(foreign.accountId to 5))

        val reported = reportsOfCompleted(foreign)

        assertEquals(CoinageTransferDetection.NotClaimed, reported.last())
        assertFalse(reported.reportsAnythingReceived())
        assertNeverSubmitted(foreign)
    }

    @Test
    fun `a mixed payment claims only the coins of the configured instance`() = runTest {
        val home = key(1)
        val foreign = key(2)
        givenConfiguredInstance(0u)
        givenLedgerFinalizesOnSubmit(home)
        givenChainSees(mapOf(home.accountId to 0, foreign.accountId to 5))

        val reported = reportsOfCompleted(home, foreign)

        assertClaimedOnce(home)
        assertNeverSubmitted(foreign)
        assertEquals(CoinageTransferDetection.ClaimedPartially(ONE_ASSET), reported.last())
        assertFalse(reported.any { it is CoinageTransferDetection.Claimed })
    }

    private fun assertClaimedOnce(coin: PeerCoin) {
        coVerify(exactly = 1) { submissionUseCase(any(), any(), any(), any()) }
        coVerify(exactly = 1) { submissionUseCase(listOf(coin.privateKey), any(), groupId, RETRY_UNTIL) }
    }

    private fun assertNeverSubmitted(coin: PeerCoin) {
        val key = coin.privateKey
        val accountId = coin.accountId

        coVerify(exactly = 0) { submissionUseCase(match { key in it }, any(), any(), any()) }
        coVerify(exactly = 0) { submissionUseCase(any(), match { accountId in it.keys }, any(), any()) }
    }

    private fun givenConfiguredInstance(instance: UInt) {
        coEvery { coinageInstanceIdProvider.instanceId() } returns Result.success(instance)
    }

    private fun List<CoinageTransferDetection>.reportsAnythingReceived() = any {
        it is CoinageTransferDetection.Claimed || it is CoinageTransferDetection.ClaimingRest || it is CoinageTransferDetection.ClaimedPartially
    }

    private suspend fun reportsOf(vararg coins: PeerCoin): List<CoinageTransferDetection> {
        val reported = mutableListOf<CoinageTransferDetection>()

        withTimeoutOrNull(IDLE) { claimOf(*coins).collect { reported += it } }

        return reported
    }

    private suspend fun reportsOfCompleted(vararg coins: PeerCoin): List<CoinageTransferDetection> {
        val reported = mutableListOf<CoinageTransferDetection>()

        val completed = withTimeoutOrNull(IDLE) {
            claimOf(*coins).collect { reported += it }
            true
        }

        assertTrue("the claim was left open when nothing more would be attempted", completed == true)

        return reported
    }

    private fun claimOf(vararg coins: PeerCoin) = useCase.claim(coins.map { it.privateKey }, groupId, RETRY_UNTIL)

    private fun givenChainSees(look: Map<AccountId, Int>) {
        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            flow {
                emit(Result.success(look.mapValues { (_, instance) -> OnChainCoinInfo(instanceId = instance, value = 3, age = 0) }))
                awaitCancellation()
            }
        }
    }

    private fun givenLedgerFinalizesOnSubmit(coin: PeerCoin) {
        val ledger = MutableStateFlow(emptyList<CoinageTransactionState>())

        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns ledger
        coEvery { submissionUseCase(any(), any(), any(), any()) } answers {
            ledger.value = listOf(
                CoinageTransactionState(
                    id = CoinageTransactionId(1),
                    status = FINALIZED_SUCCESS,
                    inputs = listOf(CoinageInput.Coin.Received(coin.accountId)),
                    outputs = listOf(OwnAsset.Coin(testKey(0))),
                )
            )

            Result.success(Unit)
        }
    }

    private class PeerCoin(val privateKey: CoinPrivateKey) {
        val accountId: AccountId = privateKey.value.toDataByteArray()
    }

    private fun key(seed: Int): PeerCoin {
        val privateKey: CoinPrivateKey = byteArrayOf(seed.toByte()).toDataByteArray()
        val keypair: Sr25519Keypair = mockk()

        every { keypair.publicKey } returns privateKey.value
        every { privateKey.deriveKeypair() } returns keypair

        return PeerCoin(privateKey)
    }

    private companion object {
        val ONE_ASSET = BigInteger.ONE.intoBalance()

        val IDLE = 60.seconds

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)
        val WINDOW_OPEN = Instant.fromEpochSeconds(500)
        const val DERIVATION_FILE = "io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemoKt"
    }
}
