package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransactionAssets
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferMemoBuilder
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferPlanner
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferPlannerFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.InsufficientBalanceException
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.TransferSubmissionFailedException
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.ExactMatchStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.SplitCoinStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinageAssetSelector
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.SpendScope
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.CoinageSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUBMISSION
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Backs both the send and the running plan preview, so it has to reach the funds a confirmed send may use —
 * and no further than it must, or a transfer that never needed the offer spends privacy for nothing.
 */
@OptIn(ExperimentalTime::class)
class RealPrepareCoinageTransferUseCaseTest {
    private val planner: TransferPlanner = mockk()
    private val plannerFactory: TransferPlannerFactory = mockk()
    private val assetSelector: CoinageAssetSelector = mockk()
    private val memoBuilder: TransferMemoBuilder = mockk()
    private val transactionService: CoinageTransactionService = mockk()
    private val coinageTransactionFactory: CoinageTransaction.Factory = mockk()
    private val coinageTransaction: CoinageTransaction = mockk(relaxUnitFun = true)
    private val handoffCommit: CoinageHandoffCommit = mockk()

    private val useCase = RealPrepareCoinageTransferUseCase(
        assetSelector = assetSelector,
        plannerFactory = plannerFactory,
        exactMatchStrategyFactory = ExactMatchStrategyFactory(transactionService),
        splitStrategyFactory = SplitCoinStrategyFactory(transactionService, coinageTransactionFactory),
        unloadAndSplitStrategyFactory = mockk(),
        chainAssetProvider = mockk(),
        memoBuilder = memoBuilder,
        transactionService = transactionService,
    )

    private val amount: BigDecimal = BigDecimal.TEN
    private val plan = TransferPlan(StrategyType.ExactCoins(coins = emptyList()))

    // One instance each: Coin equality runs through the mocked accountId, so two "identical" coins differ.
    private val spendableCoin = coinOf(derivationIndex = 1)
    private val heldForPrivacy = coinOf(derivationIndex = 2)

    private val spendableOnly = listOf(spendableCoin)
    private val widened = listOf(spendableCoin, heldForPrivacy)

    @Test
    fun `an amount the spendable funds cover is never planned against the wider set`() = runBlocking<Unit> {
        withPlanner()
        withScopes()
        every { planner.plan(amount, spendableOnly, any()) } returns Result.success(plan)

        assertEquals(plan, useCase.preparePlan(amount).getOrNull())

        verify(exactly = 0) { planner.plan(amount, widened, any()) }
    }

    @Test
    fun `an amount the spendable funds cannot cover falls back to the wider set`() = runBlocking<Unit> {
        withPlanner()
        withScopes()
        every { planner.plan(amount, spendableOnly, any()) } returns failure()
        every { planner.plan(amount, widened, any()) } returns Result.success(plan)

        assertEquals(plan, useCase.preparePlan(amount).getOrNull())
    }

    @Test
    fun `neither set covering the amount fails`() = runBlocking<Unit> {
        withPlanner()
        withScopes()
        every { planner.plan(amount, any(), any()) } returns failure()

        assertTrue(useCase.preparePlan(amount).isFailure)
    }

    @Test
    fun `no planner means no plan`() = runBlocking<Unit> {
        withScopes()
        coEvery { plannerFactory.create() } returns failure()

        assertTrue(useCase.preparePlan(amount).isFailure)

        verify(exactly = 0) { planner.plan(any(), any(), any()) }
    }

    // ---- sending: chat payments schedule on commit, merchant payments submit before handing over ----

    @Test
    fun `a chat send completes without scheduling or building anything`() = runBlocking<Unit> {
        withSplitScheduling()

        val prepared = prepareChatSend()

        assertTrue(prepared.isSuccess)
        verifyNothingScheduled()
    }

    /**
     * The commit runs inside the transaction that saves the payment message, so registering the transactions
     * there is what ties them to the message: a saved payment always has them, a lost one never does.
     */
    @Test
    fun `a chat send schedules its transactions only when the handoff is committed`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()

        val prepared = prepareChatSend().getOrThrow()
        verifyNothingScheduled()

        assertTrue(prepared.handoffCommit.commit().isSuccess)

        coVerify(exactly = 1) { handoffCommit.commit() }
        coVerify(exactly = 1) { transactionService.scheduleTransactions(listOf(splitTransaction), any()) }
    }

    /** A refusal thrown out of the commit is what rolls the payment message back with it. */
    @Test
    fun `a scheduling refusal fails the commit`() = runBlocking<Unit> {
        withSplitScheduling()
        coEvery { transactionService.scheduleTransactions(any(), any()) } returns
            Result.failure(IllegalStateException("input already claimed"))

        val prepared = prepareChatSend().getOrThrow()

        assertTrue(prepared.handoffCommit.commit().isFailure)
    }

    @Test
    fun `an exact-coins chat send schedules nothing`() = runBlocking<Unit> {
        withExactScheduling()

        val prepared = prepareChatSend(plan).getOrThrow()

        assertTrue(prepared.handoffCommit.commit().isSuccess)
        coVerify(exactly = 1) { handoffCommit.commit() }
        verifyNothingScheduled()
    }

    /**
     * A merchant takes the keys the moment they are handed over and watches for the coins right away, so the
     * memo only leaves once every transaction is on the wire — never while one is still waiting to be built.
     */
    @Test
    fun `a merchant send schedules at once and waits until every transaction is submitted`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING_SUBMISSION)), listOf(stateOf(PENDING)))

        val prepared = prepareMerchantSend()

        assertTrue(prepared.isSuccess)
        coVerify(exactly = 1) { transactionService.scheduleTransactions(listOf(splitTransaction), any()) }
        coVerify(exactly = 0) { handoffCommit.commit() }
    }

    /** The coins stay reserved and their transactions stay scheduled; only the screen stops waiting. */
    @Test
    fun `a merchant send that is not submitted in time fails`() = runTest {
        withSplitScheduling()
        withSchedulingAccepted()
        every { transactionService.subscribeOperationGroupStatuses(any()) } returns flow {
            emit(listOf(stateOf(PENDING_SUBMISSION)))
            awaitCancellation()
        }

        val prepared = prepareMerchantSend()

        assertTrue(prepared.exceptionOrNull() is TransferSubmissionFailedException)
        coVerify(exactly = 0) { handoffCommit.commit() }
    }

    @Test
    fun `a merchant send fails when a transaction could not be submitted`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING_SUBMISSION)), listOf(stateOf(FAILURE)))

        val prepared = prepareMerchantSend()

        assertTrue(prepared.exceptionOrNull() is TransferSubmissionFailedException)
    }

    /** The memo never reaches the merchant, so its coins are released now rather than on the next launch. */
    @Test
    fun `a merchant send that could not be submitted releases its reservation`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING_SUBMISSION)), listOf(stateOf(FAILURE)))

        prepareMerchantSend()

        coVerify(exactly = 1) { handoffCommit.release() }
        coVerify(exactly = 0) { handoffCommit.commit() }
    }

    @Test
    fun `a merchant send that was submitted keeps its reservation`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING)))

        prepareMerchantSend()

        coVerify(exactly = 0) { handoffCommit.release() }
    }

    @Test
    fun `a merchant send is retried until its window`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING)))

        prepareMerchantSend()

        verifyScheduledWithPolicy(CoinageSubmissionParams.splitPolicy(RETRIED_UNTIL_WINDOW))
    }

    @Test
    fun `a chat send is retried until its window`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()

        prepareChatSend().getOrThrow().handoffCommit.commit()

        verifyScheduledWithPolicy(CoinageSubmissionParams.splitPolicy(RETRIED_UNTIL_WINDOW))
    }

    private suspend fun prepareChatSend(chatPlan: TransferPlan = splitPlan) =
        with(StalenessReportCollector.NoOp) { useCase.prepareScheduledMemo(chatPlan, RETRY_UNTIL) }

    private suspend fun prepareMerchantSend() =
        with(StalenessReportCollector.NoOp) { useCase.prepareMemo(splitPlan, RETRY_UNTIL) }

    private fun withSplitScheduling() {
        every { coinageTransactionFactory.newTransaction() } returns coinageTransaction
        coEvery { coinageTransaction.mintCoins(any(), any()) } returns Result.success(emptyList())
        every { coinageTransaction.build() } returns CoinageTransactionAssets(
            inputs = splitTransaction.inputs,
            outputs = splitTransaction.outputs,
            handedOff = emptyList(),
        )
        withHandoffAndCommit()
    }

    private fun withExactScheduling() {
        withHandoffAndCommit()
    }

    private fun withHandoffAndCommit() {
        coEvery { transactionService.preCommitHandoff(any()) } returns Result.success(handoffCommit)
        coEvery { memoBuilder.buildMemo(any()) } returns Result.success(TransferMemo(emptyList(), Balance.ZERO))
        coEvery { handoffCommit.commit() } returns Result.success(Unit)
        coEvery { handoffCommit.release() } returns Result.success(Unit)
    }

    private fun withSchedulingAccepted() {
        coEvery { transactionService.scheduleTransactions(any(), any()) } returns Result.success(listOf(CoinageTransactionId(1)))
    }

    private fun withGroupReports(vararg emissions: List<CoinageTransactionState>) {
        every { transactionService.subscribeOperationGroupStatuses(any()) } returns flowOf(*emissions)
    }

    private fun verifyNothingScheduled() {
        coVerify(exactly = 0) { transactionService.scheduleTransactions(any(), any()) }
    }

    private fun verifyScheduledWithPolicy(policy: SubmissionPolicy) {
        coVerify(exactly = 1) { transactionService.scheduleTransactions(match { it.single().policy == policy }, any()) }
    }

    private fun stateOf(status: DurableTxStatus) = CoinageTransactionState(
        id = CoinageTransactionId(1),
        status = status,
        inputs = splitTransaction.inputs,
        outputs = splitTransaction.outputs,
    )

    private fun withPlanner() {
        coEvery { plannerFactory.create() } returns Result.success(planner)
    }

    private fun withScopes() {
        coEvery { assetSelector.getSelectableCoinsByScope() } returns
            mapOf(SpendScope.SPENDABLE to spendableOnly, SpendScope.WITH_CONFIRMATION to widened)
        coEvery { assetSelector.getSelectableVouchersByScope() } returns SpendScope.entries.associateWith { emptyList() }
    }

    private fun <T> failure(): Result<T> = Result.failure(InsufficientBalanceException())

    private val splitPlan get() = TransferPlan(
        StrategyType.Split(
            splitFrom = spendableCoin,
            recipientDenominations = emptyList(),
            changeDenominations = emptyList(),
            exactCoins = emptyList(),
        )
    )

    private val splitTransaction = CoinageScheduledTransactionRequest(
        policy = CoinageSubmissionParams.splitPolicy(RETRIED_UNTIL_WINDOW),
        inputs = listOf(CoinageInput.Coin.Own(testKey(1))),
        outputs = listOf(OwnAsset.Coin(testKey(2))),
    )

    private companion object {
        val RETRY_UNTIL: Instant = Instant.fromEpochSeconds(1_000)
        val RETRIED_UNTIL_WINDOW = TransferSubmissionParams(buildUntil = RETRY_UNTIL, retryFailures = true)
    }

    private fun coinOf(derivationIndex: Int) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(3),
        isOnChain = true,
        accountId = mockk(),
        provenance = CoinProvenance.UNKNOWN,
    )
}
