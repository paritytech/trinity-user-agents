package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.common.centsToDollar
import io.paritytech.polkadotapp.feature_coinage_impl.common.coinageTestPrecision
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.RealCoinAmountBreakdownContext
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransactionAssets
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigInteger
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class UnloadAndSplitVouchersStrategyTest {
    private val transactionService = mockk<CoinageTransactionService>()
    private val coinRepository = mockk<CoinRepository>(relaxed = true)
    private val transaction = mockk<CoinageTransaction>(relaxed = true)
    private val breakdownUseCase = mockk<CoinAmountBreakdownUseCase>()
    private val balanceConverterUseCase = mockk<CoinageBalanceConverterUseCase>(relaxed = true)
    private val handoffCommit = mockk<CoinageHandoffCommit>()
    private val handedOff = listOf(OwnAsset.Coin(testKey(2)))

    private val strategy = UnloadAndSplitVouchersStrategy(
        payload = StrategyType.UnloadAndSplit(
            vouchersToUnload = listOf(voucherInRecycler(index = 1)),
            recipientAmount = 32.0.centsToDollar(),
            exactCoins = emptyList(),
        ),
        chain = mockk(relaxed = true),
        coinRepository = coinRepository,
        transactionService = transactionService,
        coinageTransactionFactory = mockk { every { newTransaction() } returns transaction },
        breakdownUseCase = breakdownUseCase,
        balanceConverterUseCase = balanceConverterUseCase,
    )

    @Test
    fun `a schedule cancelled while it waits is a cancellation, not a failed result`() = runBlocking<Unit> {
        withBreakdownHeld()

        var result: Result<ScheduledTransfer>? = null
        val scheduling = launch { result = schedule(CoinReservationMode.PRECOMMIT) }
        yield()

        scheduling.cancelAndJoin()

        assertTrue("the scheduling was not cancelled", scheduling.isCancelled)
        assertNull("a cancelled scheduling produced a result", result)
        coVerify(exactly = 0) { transactionService.preCommitHandoff(any()) }
    }

    @Test
    fun `prepares the handoff of its outputs when scheduled in PREPARE mode`() = runBlocking<Unit> {
        withVoucherBatchMinted()
        withHandoffAccepted()

        schedule(CoinReservationMode.PREPARE)

        verifyHandoffPrepared()
    }

    @Test
    fun `pre-commits the handoff of its outputs when scheduled in PRECOMMIT mode`() = runBlocking<Unit> {
        withVoucherBatchMinted()
        withHandoffAccepted()

        schedule(CoinReservationMode.PRECOMMIT)

        verifyHandoffPreCommitted()
    }

    private suspend fun schedule(mode: CoinReservationMode) =
        with(StalenessReportCollector.NoOp) { strategy.schedule(PARAMS, mode) }

    private fun withBreakdownHeld() {
        coEvery { breakdownUseCase.createCoinAmountBreakdown() } coAnswers { awaitCancellation() }
    }

    private fun withVoucherBatchMinted() {
        coEvery { breakdownUseCase.createCoinAmountBreakdown() } returns
            Result.success(RealCoinAmountBreakdownContext(coinageTestPrecision, testConversionContext, ALLOWED_EXPONENTS))
        coEvery { balanceConverterUseCase.create() } returns Result.success(testConversionContext)
        coEvery { coinRepository.fetchMaxConsolidation(any()) } returns Result.success(MAX_CONSOLIDATION)
        coEvery { transaction.mintCoins(any(), any()) } returns Result.success(emptyList())
        every { transaction.build() } returns CoinageTransactionAssets(emptyList(), emptyList(), handedOff)
    }

    private fun withHandoffAccepted() {
        coEvery { transactionService.prepareHandoff(any()) } returns Result.success(handoffCommit)
        coEvery { transactionService.preCommitHandoff(any()) } returns Result.success(handoffCommit)
    }

    private fun verifyHandoffPrepared() {
        coVerify { transactionService.prepareHandoff(handedOff) }
        coVerify(exactly = 0) { transactionService.preCommitHandoff(any()) }
    }

    private fun verifyHandoffPreCommitted() {
        coVerify { transactionService.preCommitHandoff(handedOff) }
        coVerify(exactly = 0) { transactionService.prepareHandoff(any()) }
    }

    private fun voucherInRecycler(index: Int) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(index),
        ringVrfPublicKey = byteArrayOf(index.toByte()).toDataByteArray(),
        recyclerValue = ValueExponent(5),
        location = RecyclerVoucher.Location.InRecycler(RingIndex(BigInteger.ZERO), recyclerMembers = 1, enteredAt = null),
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private companion object {
        const val MAX_CONSOLIDATION = 64
        val ALLOWED_EXPONENTS = (-2..7).map { ValueExponent(it) }.toSet()
        val PARAMS = TransferSubmissionParams(buildUntil = Instant.DISTANT_FUTURE, retryFailures = false)
    }
}
