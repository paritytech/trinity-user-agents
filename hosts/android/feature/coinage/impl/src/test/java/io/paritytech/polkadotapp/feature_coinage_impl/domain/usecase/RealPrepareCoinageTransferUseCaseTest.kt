package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
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
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferMemoBuilder
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferPlanner
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferPlannerFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.InsufficientBalanceException
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.TransferSubmissionFailedException
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.ExactMatchStrategy
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.ExactMatchStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.ScheduledTransfer
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.SplitCoinStrategy
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.SplitCoinStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinageAssetSelector
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.SpendScope
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUBMISSION
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Backs both the send and the running plan preview, so it has to reach the funds a confirmed send may use —
 * and no further than it must, or a transfer that never needed the offer spends privacy for nothing.
 */
@OptIn(ExperimentalTime::class)
class RealPrepareCoinageTransferUseCaseTest {
    private val planner: TransferPlanner = mock()
    private val plannerFactory: TransferPlannerFactory = mock()
    private val assetSelector: CoinageAssetSelector = mock()
    private val exactMatchStrategyFactory: ExactMatchStrategyFactory = mock()
    private val splitStrategyFactory: SplitCoinStrategyFactory = mock()
    private val memoBuilder: TransferMemoBuilder = mock()
    private val transactionService: CoinageTransactionService = mock()

    private val splitStrategy: SplitCoinStrategy = mock()
    private val exactStrategy: ExactMatchStrategy = mock()
    private val handoffCommit: CoinageHandoffCommit = mock()

    private val useCase = RealPrepareCoinageTransferUseCase(
        assetSelector = assetSelector,
        plannerFactory = plannerFactory,
        exactMatchStrategyFactory = exactMatchStrategyFactory,
        splitStrategyFactory = splitStrategyFactory,
        unloadAndSplitStrategyFactory = mock(),
        chainAssetProvider = mock(),
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
        whenever(planner.plan(eq(amount), eq(spendableOnly), any())).thenReturn(Result.success(plan))

        assertEquals(plan, useCase.preparePlan(amount).getOrNull())

        verify(planner, never()).plan(eq(amount), eq(widened), any())
    }

    @Test
    fun `an amount the spendable funds cannot cover falls back to the wider set`() = runBlocking<Unit> {
        withPlanner()
        withScopes()
        whenever(planner.plan(eq(amount), eq(spendableOnly), any())).thenReturn(failure())
        whenever(planner.plan(eq(amount), eq(widened), any())).thenReturn(Result.success(plan))

        assertEquals(plan, useCase.preparePlan(amount).getOrNull())
    }

    @Test
    fun `neither set covering the amount fails`() = runBlocking<Unit> {
        withPlanner()
        withScopes()
        whenever(planner.plan(eq(amount), any(), any())).thenReturn(failure())

        assertTrue(useCase.preparePlan(amount).isFailure)
    }

    @Test
    fun `no planner means no plan`() = runBlocking<Unit> {
        withScopes()
        whenever(plannerFactory.create()).thenReturn(failure())

        assertTrue(useCase.preparePlan(amount).isFailure)

        verify(planner, never()).plan(any(), any(), any())
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

        verify(handoffCommit).commit()
        verify(transactionService).scheduleTransactions(eq(listOf(splitTransaction)), anyGroupId())
    }

    /** A refusal thrown out of the commit is what rolls the payment message back with it. */
    @Test
    fun `a scheduling refusal fails the commit`() = runBlocking<Unit> {
        withSplitScheduling()
        whenever(transactionService.scheduleTransactions(any(), anyGroupId()))
            .thenReturn(Result.failure(IllegalStateException("input already claimed")))

        val prepared = prepareChatSend().getOrThrow()

        assertTrue(prepared.handoffCommit.commit().isFailure)
    }

    @Test
    fun `an exact-coins chat send schedules nothing`() = runBlocking<Unit> {
        withExactScheduling()

        val prepared = prepareChatSend(plan).getOrThrow()

        assertTrue(prepared.handoffCommit.commit().isSuccess)
        verify(handoffCommit).commit()
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
        verify(transactionService).scheduleTransactions(eq(listOf(splitTransaction)), anyGroupId())
        verify(handoffCommit, never()).commit()
    }

    /** The coins stay reserved and their transactions stay scheduled; only the screen stops waiting. */
    @Test
    fun `a merchant send that is not submitted in time fails`() = runTest {
        withSplitScheduling()
        withSchedulingAccepted()
        whenever(transactionService.subscribeOperationGroupStatuses(anyGroupId())).thenReturn(
            flow {
                emit(listOf(stateOf(PENDING_SUBMISSION)))
                awaitCancellation()
            }
        )

        val prepared = prepareMerchantSend()

        assertTrue(prepared.exceptionOrNull() is TransferSubmissionFailedException)
        verify(handoffCommit, never()).commit()
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
        whenever(handoffCommit.release()).thenReturn(Result.success(Unit))

        prepareMerchantSend()

        verify(handoffCommit).release()
        verify(handoffCommit, never()).commit()
    }

    @Test
    fun `a merchant send that was submitted keeps its reservation`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING)))

        prepareMerchantSend()

        verify(handoffCommit, never()).release()
    }

    @Test
    fun `a merchant send is retried until its window`() = runBlocking<Unit> {
        withSplitScheduling()
        withSchedulingAccepted()
        withGroupReports(listOf(stateOf(PENDING)))

        prepareMerchantSend()

        val verified = verify(splitStrategy)
        with(eq(StalenessReportCollector.NoOp)) {
            verified.schedule(eq(TransferSubmissionParams(buildUntil = RETRY_UNTIL, retryFailures = true)))
        }
    }

    @Test
    fun `a chat send is retried until its window`() = runBlocking<Unit> {
        withSplitScheduling()

        prepareChatSend()

        val verified = verify(splitStrategy)
        with(eq(StalenessReportCollector.NoOp)) {
            verified.schedule(eq(TransferSubmissionParams(buildUntil = RETRY_UNTIL, retryFailures = true)))
        }
    }

    private suspend fun prepareChatSend(chatPlan: TransferPlan = splitPlan) =
        with(StalenessReportCollector.NoOp) { useCase.prepareScheduledMemo(chatPlan, RETRY_UNTIL) }

    private suspend fun prepareMerchantSend() =
        with(StalenessReportCollector.NoOp) { useCase.prepareMemo(splitPlan, RETRY_UNTIL) }

    private suspend fun withSplitScheduling() {
        whenever(splitStrategyFactory.create(any())).thenReturn(splitStrategy)
        whenever(with(eq(StalenessReportCollector.NoOp)) { splitStrategy.schedule(any()) })
            .thenReturn(Result.success(ScheduledTransfer(emptyList(), handoffCommit, listOf(splitTransaction))))
        withMemoAndCommit()
    }

    private suspend fun withExactScheduling() {
        whenever(exactMatchStrategyFactory.create(any())).thenReturn(exactStrategy)
        whenever(with(eq(StalenessReportCollector.NoOp)) { exactStrategy.schedule(any()) })
            .thenReturn(Result.success(ScheduledTransfer(emptyList(), handoffCommit, emptyList())))
        withMemoAndCommit()
    }

    private suspend fun withMemoAndCommit() {
        whenever(memoBuilder.buildMemo(any())).thenReturn(Result.success(TransferMemo(emptyList(), Balance.ZERO)))
        whenever(handoffCommit.commit()).thenReturn(Result.success(Unit))
    }

    private suspend fun withSchedulingAccepted() {
        whenever(transactionService.scheduleTransactions(any(), anyGroupId()))
            .thenReturn(Result.success(listOf(CoinageTransactionId(1))))
    }

    private fun withGroupReports(vararg emissions: List<CoinageTransactionState>) {
        whenever(transactionService.subscribeOperationGroupStatuses(anyGroupId())).thenReturn(flowOf(*emissions))
    }

    private suspend fun verifyNothingScheduled() {
        verify(transactionService, never()).scheduleTransactions(any(), anyGroupId())
    }

    private fun stateOf(status: DurableTxStatus) = CoinageTransactionState(
        id = CoinageTransactionId(1),
        status = status,
        inputs = splitTransaction.inputs,
        outputs = splitTransaction.outputs,
    )

    /** A value class over String: the matcher is what counts, the wrapped value only has to be non-null. */
    private fun anyGroupId() = CoinageOperationGroupId(Mockito.any<String>() ?: "")

    private suspend fun withPlanner() {
        whenever(plannerFactory.create()).thenReturn(Result.success(planner))
    }

    private suspend fun withScopes() {
        whenever(assetSelector.getSelectableCoinsByScope()).thenReturn(
            mapOf(SpendScope.SPENDABLE to spendableOnly, SpendScope.WITH_CONFIRMATION to widened)
        )
        whenever(assetSelector.getSelectableVouchersByScope()).thenReturn(
            SpendScope.entries.associateWith { emptyList() }
        )
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
        policy = SubmissionPolicy(SubmissionPolicyId("split"), byteArrayOf().toDataByteArray()),
        inputs = listOf(CoinageInput.Coin.Own(testKey(1))),
        outputs = listOf(OwnAsset.Coin(testKey(2))),
    )

    private companion object {
        val RETRY_UNTIL: Instant = Instant.fromEpochSeconds(1_000)
    }

    private fun coinOf(derivationIndex: Int) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(3),
        isOnChain = true,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )
}
