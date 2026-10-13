package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.math.BigDecimal
import java.math.BigInteger
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Onboarding an amount into the recycler, and retrying the vouchers that did not take.
 *
 * A voucher whose transaction failed minted nothing, so the money it stood for is still in the funding
 * account and still owed — worth retrying for as long as the caller's window allows. What the tests here pin
 * is when a retry may happen at all, because the two ways to get it wrong are opposite and both expensive.
 * Onboarding a denomination twice charges the account for money the user never receives; giving up on one
 * leaves the top-up short with no way to ask for the rest, since the key that funds it is one-time.
 */
@OptIn(ExperimentalTime::class)
class RealOnboardingUseCaseTest {
    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val voucherRepository: VoucherRepository = mockk()
    private val transactionService: CoinageTransactionService = mockk()
    private val breakdownUseCase: CoinAmountBreakdownUseCase = mockk()
    private val balanceConverterUseCase: CoinageBalanceConverterUseCase = mockk()
    private val assetValueUseCase: CoinageAssetValueUseCase = mockk()
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry = mockk()
    private val submissionUseCase: CoinageOnboardingSubmissionUseCase = mockk()
    private val timeProvider: TimeProvider = mockk()

    private val useCase = RealOnboardingUseCase(
        chainAssetProvider = chainAssetProvider,
        voucherRepository = voucherRepository,
        transactionService = transactionService,
        coinAmountBreakdownUseCase = breakdownUseCase,
        balanceConverterUseCase = balanceConverterUseCase,
        assetValueUseCase = assetValueUseCase,
        tokenBalanceTypeRegistry = tokenBalanceTypeRegistry,
        submissionUseCase = submissionUseCase,
        timeProvider = timeProvider,
    )

    private val groupId = CoinageOperationGroupId("group")

    /** Which denomination each voucher the ledger reports was minted for. */
    private val voucherDenominations = mutableMapOf<Int, ValueExponent>()
    private var nextVoucherIndex = 0

    @Before
    fun openTheWindow() {
        // A real Chain, not a bare mock: the loop renders the funding account as an address for the log,
        // and an unstubbed property there is swallowed by the balance flow's retry rather than surfacing.
        coEvery { chainAssetProvider.chain() } returns mockk {
            every { isEthereumBased } returns false
            every { addressPrefix } returns 42
        }
        coEvery { chainAssetProvider.asset() } returns mockk()
        every { timeProvider.now() } returns WINDOW_OPEN
        coEvery { submissionUseCase(any(), any(), any(), any()) } returns Result.success(Unit)
        coEvery { balanceConverterUseCase.create() } returns Result.success(PowerOfTwoPricing)
        coEvery { voucherRepository.getByRingVrfKeyIndices(any()) } answers {
            firstArg<List<CoinageKeyIndex>>().mapNotNull { key -> voucherDenominations[key.item]?.let { voucher(key.item, it) } }
        }
        coEvery { assetValueUseCase.valueOf(any()) } answers {
            // One unit per minted output, so what an onboarding is said to be worth stays visible in assertions.
            Result.success(BigInteger.valueOf(firstArg<List<OwnAsset>>().size.toLong()).intoBalance())
        }
    }

    // ---- when onboarding may be submitted ----

    /** A first attempt: the group is empty and the account holds the whole amount. Both vouchers go at once. */
    @Test
    fun `a funded account onboards every denomination the amount breaks into`() = runTest {
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(price(listOf(BIG, SMALL))))

        reportsOf()

        assertOnboardedOnce(listOf(BIG, SMALL))
    }

    /**
     * The voucher the previous attempt registered failed, so it minted nothing and the money it stood for is
     * still in the account. It is submitted again — as a fresh voucher, because the ledger will not take an
     * output any entry has already minted.
     */
    @Test
    fun `a voucher that failed is onboarded again while the account can still cover it`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(FAILURE, SMALL)))
        givenAccountHolds(listOf(price(listOf(SMALL))))

        reportsOf()

        assertOnboardedOnce(listOf(SMALL))
    }

    /**
     * A voucher of ours already finalized for this denomination, so the amount is onboarded. Submitting
     * again would charge the account a second time for money the user has already been credited.
     */
    @Test
    fun `a denomination a finalized voucher already minted is never onboarded again`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, SMALL)))
        givenAccountHolds(listOf(price(listOf(SMALL))))

        reportsOf()

        assertNothingOnboarded()
    }

    /**
     * An earlier attempt failed but a later one is still in flight. The denomination is unfinalized because
     * that attempt has not executed yet — not because it needs onboarding again.
     */
    @Test
    fun `a failed voucher is not onboarded again while another attempt is still live`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(FAILURE, SMALL), entry(PENDING, SMALL)))
        givenAccountHolds(listOf(price(listOf(SMALL))))

        reportsOf()

        assertNothingOnboarded()
    }

    /** Nothing to onboard from: submitting against a balance the account does not hold only gets it refused. */
    @Test
    fun `an empty account onboards nothing`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(Balance.ZERO))

        reportsOf()

        assertNothingOnboarded()
    }

    /**
     * The funding transfer had not landed when the top-up was asked for, so the first look found an empty
     * account. The money arrives a few blocks later and is onboarded then, without anything having to poll.
     */
    @Test
    fun `an account funded later is onboarded when it is`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries(), noEntries())
        givenAccountHolds(listOf(Balance.ZERO, price(listOf(SMALL))))

        reportsOf()

        assertOnboardedOnce(listOf(SMALL))
    }

    /**
     * The account covers one denomination and the transfer for the rest is still landing. Onboarding waits
     * for the whole amount rather than taking what it can see, and then submits both together.
     *
     * Splitting an amount across two batches for want of a few blocks' patience puts a second nonce sequence
     * and a second registration in the way of a top-up that was always going to be affordable.
     */
    @Test
    fun `onboarding holds out for the whole amount before submitting`() = runTest {
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(price(listOf(BIG)), price(listOf(BIG, SMALL))))

        reportsOf()

        assertOnboardedOnce(listOf(BIG, SMALL))
    }

    /**
     * An amount that breaks into two vouchers of the same denomination, one of which has already finalized.
     *
     * What is still owed is the other one. Subtracting denominations as a set would cancel both against the
     * one that landed and call a half-onboarded amount finished — and the shortfall would never be
     * submitted, because nothing would think anything was outstanding.
     */
    @Test
    fun `a denomination owed twice is only struck off once per voucher that minted it`() = runTest {
        givenAmountBreaksInto(listOf(SMALL, SMALL))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, SMALL)))
        givenAccountHolds(listOf(price(listOf(SMALL))))

        reportsOf()

        assertOnboardedOnce(listOf(SMALL))
    }

    /** And it stays open, because half of what was asked for has not been onboarded. */
    @Test
    fun `an amount owed twice over is not finished by one of the two landing`() = runTest {
        givenAmountBreaksInto(listOf(SMALL, SMALL))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        assertDoesNotComplete()
    }

    /**
     * The account was underfunded from the start and stays that way. What it can cover is onboarded rather
     * than held hostage to the part that never arrives — and the largest denomination goes first, so the
     * most value moves.
     */
    @Test
    fun `an account that stays short onboards as much as it covers`() = runTest {
        givenAmountBreaksInto(listOf(SMALL, BIG))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(price(listOf(BIG))))

        reportsOf()

        assertOnboardedOnce(listOf(BIG))
    }

    /**
     * The account is seen covering the larger denomination, then something else spends from it and the next
     * look covers only the smaller one. Neither look covers the whole amount, and what is onboarded is what
     * the account holds now.
     *
     * Submitting against the best balance ever seen would submit against money the account no longer has,
     * which is exactly the refusal this loop exists to recover from.
     */
    @Test
    fun `money the account no longer holds is not onboarded against`() = runTest {
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(price(listOf(BIG)), price(listOf(SMALL))))

        reportsOf()

        assertOnboardedOnce(listOf(SMALL))
    }

    /**
     * A voucher is in flight and the balance is looked at again — it still reads full, because the
     * transaction that will spend it has not executed yet.
     *
     * Waiting for the ledger to settle before submitting again is the whole protection against onboarding a
     * denomination twice; a fresh look at the balance is not a reason to submit while an attempt is
     * outstanding.
     */
    @Test
    fun `a denomination is not onboarded again while its voucher is still in flight`() = runTest {
        val submitted = CompletableDeferred<Unit>()

        givenAmountBreaksInto(listOf(SMALL))
        givenLedgerRegistersOnSubmit(PENDING, SMALL, signal = submitted)
        givenAccountIsSeenAgainAfter(submitted, price(listOf(SMALL)))

        reportsOf()

        coVerify(exactly = 1) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * The ledger refused the registration, so nothing was recorded and the account is untouched. The next
     * look has to try again — a refusal that quietly counted as an attempt would leave the top-up short
     * exactly as surely as never trying at all.
     */
    @Test
    fun `a submission the ledger refuses is tried again on the next look`() = runTest {
        val refused = CompletableDeferred<Unit>()

        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries())
        givenSubmissionSignals(refused, Result.failure(IllegalStateException("refused")))
        givenAccountIsSeenAgainAfter(refused, price(listOf(SMALL)))

        reportsOf()

        coVerify(exactly = 2) { submissionUseCase(any(), any(), any(), any()) }
    }

    // ---- when the retrying stops ----

    /**
     * Every denomination has a finalized voucher, so there is nothing left that retrying could win.
     * Inclusion would not have been enough: a fork can take a block away, and an onboarding closed on an
     * inclusion that is later retracted is one nothing will ever try again.
     */
    @Test
    fun `onboarding ends once every denomination has a finalized voucher`() = runTest {
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, BIG), entry(FINALIZED_SUCCESS, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOfCompleted()

        assertEquals(CoinageTransferDetection.Claimed(TWO_ASSETS, finalized = true), reported.last())
    }

    /** A voucher in a block is reported as onboarded, but it is not yet a reason to stop watching it. */
    @Test
    fun `a voucher only included in a block does not end the onboarding`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(PENDING_SUCCESS, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        assertDoesNotComplete()
    }

    /**
     * The window closed and the funding transfer never arrived. Nothing more will be tried and the caller is
     * told so, because waiting on money that has not appeared is the one thing a timer can settle.
     */
    @Test
    fun `onboarding ends when the window closes on an account that was never funded`() = runTest {
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(FAILURE, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOfCompleted()

        assertEquals(CoinageTransferDetection.NotClaimed, reported.last())
        assertNothingOnboarded()
    }

    /**
     * The deliberate divergence from claiming a peer's coins, which does attempt once however late it is
     * asked to.
     *
     * These funds are the caller's own, sitting in a one-time account nothing else will spend, and the
     * window closing is what tells the product its top-up terminally failed. Onboarding anyway would charge
     * an account whose owner has already been told the money is not coming — and the product has no way to
     * ask for it back, because it cannot re-submit against a key that is spent.
     */
    @Test
    fun `a closed window ends the onboarding even on a first attempt`() = runTest {
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(price(listOf(SMALL))))

        val reported = reportsOfCompleted()

        assertEquals(CoinageTransferDetection.NotClaimed, reported.last())
        assertNothingOnboarded()
    }

    /** An account that is not funded yet keeps its chance for as long as the window is open. */
    @Test
    fun `an account that is not funded yet keeps the onboarding open`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(Balance.ZERO))

        assertDoesNotComplete()
    }

    /** A submission the ledger refuses is not the end of it — the money is still in the account. */
    @Test
    fun `a submission the ledger refuses leaves the onboarding open`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(price(listOf(SMALL))))
        coEvery { submissionUseCase(any(), any(), any(), any()) } returns Result.failure(IllegalStateException("refused"))

        assertDoesNotComplete()
    }

    // ---- what the caller is told ----

    @Test
    fun `an onboarding reports itself as detecting before it knows anything`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(noEntries())
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOf()

        assertEquals(CoinageTransferDetection.Detecting, reported.first())
    }

    /**
     * An amount whose denominations cannot be worked out at all — that needs chain metadata, which may not
     * be to hand. Nothing can be planned, so nothing can be attempted, and the caller is told immediately
     * rather than left waiting out a window that could never produce anything.
     */
    @Test
    fun `an amount that cannot be broken down is reported as not onboarded`() = runTest {
        coEvery { breakdownUseCase.createCoinAmountBreakdown() } returns Result.failure(IllegalStateException("no metadata"))

        val reported = reportsOfCompleted()

        assertEquals(listOf(CoinageTransferDetection.Detecting, CoinageTransferDetection.NotClaimed), reported)
        assertNothingOnboarded()
    }

    /**
     * A batch landing one voucher at a time, with nothing wrong.
     *
     * It reports as still onboarding, and deliberately says nothing about how much has landed yet: on the
     * happy path vouchers land one by one, so reporting the running total would walk the user through
     * "onboarded 0.08, waiting" then "onboarded 0.12, waiting" for a top-up that is simply in progress.
     */
    @Test
    fun `an onboarding whose vouchers are merely still in flight reports as onboarding`() = runTest {
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(listOf(entry(PENDING_SUCCESS, BIG), entry(PENDING, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOf()

        assertEquals(CoinageTransferDetection.Claiming, reported.last())
    }

    /**
     * The same shape, except the denomination that has not landed had its voucher refused, so it is waiting
     * on a retry rather than simply on a block.
     *
     * Now the amount is worth reporting: part of the top-up is settled and the rest genuinely hangs in the
     * balance.
     */
    @Test
    fun `an onboarding held up by a failed voucher reports what has landed`() = runTest {
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(listOf(entry(PENDING_SUCCESS, BIG), entry(FAILURE, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOf()

        assertEquals(CoinageTransferDetection.ClaimingRest(ONE_ASSET), reported.last())
    }

    /**
     * A fork takes away the block a voucher was seen in, so the ledger lowers it back to pending. The report
     * has to follow the ledger down again rather than leave the caller told the money is onboarded.
     */
    @Test
    fun `a voucher retracted by a fork stops being reported as onboarded`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        val voucher = entry(PENDING_SUCCESS, SMALL)
        givenGroupReports(listOf(voucher), listOf(voucher.copy(status = PENDING)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOf()

        assertEquals(
            listOf(
                CoinageTransferDetection.Detecting,
                CoinageTransferDetection.Claimed(ONE_ASSET, finalized = false),
                CoinageTransferDetection.Claiming,
            ),
            reported,
        )
    }

    /**
     * A retry left the group holding the dead voucher of the attempt that failed. It was never minted on
     * chain, so counting it would report a top-up as worth more than it is.
     */
    @Test
    fun `the reported amount does not count a voucher that failed`() = runTest {
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(FAILURE, SMALL), entry(PENDING_SUCCESS, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOf()

        assertEquals(CoinageTransferDetection.Claimed(ONE_ASSET, finalized = false), reported.last())
    }

    /**
     * Two denominations and only one voucher executes before the window closes. What that voucher minted is
     * the user's regardless of what happened to its sibling.
     */
    @Test
    fun `one voucher succeeding onboards part of the amount even though its sibling failed`() = runTest {
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenAmountBreaksInto(listOf(BIG, SMALL))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, BIG), entry(FAILURE, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOfCompleted()

        assertEquals(CoinageTransferDetection.ClaimedPartially(ONE_ASSET), reported.last())
    }

    /**
     * The onboarded vouchers cannot be valued — that needs chain metadata, which may not be to hand.
     *
     * The onboarding still reports where it got to, for nothing. Saying nothing would leave a top-up that
     * actually landed looking as though it never did.
     */
    @Test
    fun `an onboarding whose vouchers cannot be valued still reports that they arrived`() = runTest {
        coEvery { assetValueUseCase.valueOf(any()) } returns Result.failure(IllegalStateException("no metadata"))
        givenAmountBreaksInto(listOf(SMALL))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, SMALL)))
        givenAccountHolds(listOf(Balance.ZERO))

        val reported = reportsOfCompleted()

        assertEquals(CoinageTransferDetection.Claimed(Balance.ZERO, finalized = true), reported.last())
    }

    // ---- harness ----

    /**
     * Exactly one submission was made, and it carried exactly these denominations.
     *
     * Both halves matter: `exactly = 1` on its own counts only the calls that match, so a second submission
     * of a different shape — the very thing double-onboarding looks like — would go unnoticed.
     */
    private fun assertOnboardedOnce(denominationsLargestFirst: List<ValueExponent>) {
        coVerify(exactly = 1) { submissionUseCase(any(), any(), any(), any()) }
        coVerify(exactly = 1) { submissionUseCase(denominationsLargestFirst, any(), any(), groupId) }
    }

    private fun assertNothingOnboarded() {
        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
    }

    private fun onboarding(): Flow<CoinageTransferDetection> =
        useCase.onboardDurably(AMOUNT, signerSource(), groupId, RETRY_UNTIL)

    /**
     * Everything the onboarding says before it either finishes or runs out of anything to react to.
     *
     * An onboarding is driven by two subscriptions, so how many times it re-evaluates is the combining's
     * business and not something a test should pin. What is worth pinning is what it ends up reporting.
     */
    private suspend fun reportsOf(): List<CoinageTransferDetection> {
        val reported = mutableListOf<CoinageTransferDetection>()

        withTimeoutOrNull(IDLE) { onboarding().collect { reported += it } }

        return reported
    }

    /** Drains the onboarding and insists it actually finished — that nothing more would ever be attempted. */
    private suspend fun reportsOfCompleted(): List<CoinageTransferDetection> {
        val reported = mutableListOf<CoinageTransferDetection>()

        val completed = withTimeoutOrNull(IDLE) {
            onboarding().collect { reported += it }
            true
        }

        assertTrue("the onboarding was left open when nothing more would be attempted", completed == true)

        return reported
    }

    private suspend fun assertDoesNotComplete() {
        var completed = false

        withTimeoutOrNull(IDLE) {
            onboarding().collect { }
            completed = true
        }

        assertFalse("the onboarding was closed while there was still something to onboard", completed)
    }

    private fun signerSource(): TransactionSignerSource.Signed {
        val metaAccount: MetaAccount = mockk()
        every { metaAccount.accountIdIn(any<Chain>()) } returns ACCOUNT_ID

        return TransactionSignerSource.FromAccount(metaAccount)
    }

    private fun givenAmountBreaksInto(denominations: List<ValueExponent>) {
        val breakdown = object : CoinAmountBreakdown {
            override fun breakdown(amount: BigDecimal) = denominations

            override fun roundDownAmount(amount: BigDecimal) = amount

            override fun remainderAfterBreakdown(amount: BigDecimal) = Balance.ZERO
        }

        coEvery { breakdownUseCase.createCoinAmountBreakdown() } returns Result.success(breakdown)
    }

    /**
     * What the funding account holds, one emission per look. A storage subscription stays open, so the flow
     * does not end after the last emission — which is what lets an onboarding stay open waiting for money.
     */
    private fun givenAccountHolds(looks: List<Balance>) {
        givenAccountBalanceEmits {
            looks.forEach { emit(balanceUpdate(it)) }
            awaitCancellation()
        }
    }

    /** A second look at an unchanged account, taken only once a submission has been attempted. */
    private fun givenAccountIsSeenAgainAfter(attempted: CompletableDeferred<Unit>, holds: Balance) {
        givenAccountBalanceEmits {
            emit(balanceUpdate(holds))
            attempted.await()
            emit(balanceUpdate(holds))
            awaitCancellation()
        }
    }

    private fun givenAccountBalanceEmits(
        emissions: suspend FlowCollector<AccountBalanceUpdate>.() -> Unit,
    ) {
        val balanceType: TokenBalanceType = mockk()

        every { tokenBalanceTypeRegistry.typeFor(any()) } returns balanceType
        coEvery { balanceType.subscribeAccountBalanceUpdates(any(), any()) } returns flow(emissions)
    }

    /** A ledger that records what it is handed, so a submission changes what the next pass reads. */
    private fun givenLedgerRegistersOnSubmit(
        status: DurableTxStatus,
        denomination: ValueExponent,
        signal: CompletableDeferred<Unit>,
    ) {
        val ledger = MutableStateFlow(noEntries())

        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns ledger
        coEvery { submissionUseCase(any(), any(), any(), any()) } answers {
            ledger.value = listOf(entry(status, denomination))
            signal.complete(Unit)

            Result.success(Unit)
        }
    }

    private fun givenSubmissionSignals(signal: CompletableDeferred<Unit>, outcome: Result<Unit>) {
        coEvery { submissionUseCase(any(), any(), any(), any()) } answers {
            signal.complete(Unit)
            outcome
        }
    }

    private fun givenGroupReports(vararg emissions: List<CoinageTransactionState>) {
        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns flow {
            emissions.forEach { emit(it) }
            awaitCancellation()
        }
    }

    private fun noEntries() = emptyList<CoinageTransactionState>()

    /** One registered voucher, minted for [denomination] — the shape onboarding always registers. */
    private fun entry(status: DurableTxStatus, denomination: ValueExponent): CoinageTransactionState {
        val index = nextVoucherIndex++
        voucherDenominations[index] = denomination

        return CoinageTransactionState(
            id = CoinageTransactionId(index.toLong()),
            status = status,
            inputs = emptyList(),
            outputs = listOf(OwnAsset.Voucher(testKey(index))),
        )
    }

    private fun voucher(index: Int, denomination: ValueExponent) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(index),
        ringVrfPublicKey = byteArrayOf(index.toByte()).toDataByteArray(),
        recyclerValue = denomination,
        location = RecyclerVoucher.Location.Unknown,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = null,
    )

    private fun balanceUpdate(transferable: Balance): AccountBalanceUpdate {
        val balance: TokenBalance = mockk()
        every { balance.transferable } returns transferable

        return AccountBalanceUpdate(updatedAt = null, balance = balance)
    }

    private fun price(denominations: List<ValueExponent>): Balance =
        denominations.fold(Balance.ZERO) { total, it -> total + PowerOfTwoPricing.formatExponentToBalance(it) }

    private companion object {
        val SMALL = ValueExponent(0)
        val BIG = ValueExponent(3)

        val ONE_ASSET = BigInteger.ONE.intoBalance()
        val TWO_ASSETS = BigInteger.TWO.intoBalance()

        val ACCOUNT_ID: AccountId = byteArrayOf(7).toDataByteArray()
        val AMOUNT: BigDecimal = BigDecimal.ONE

        val IDLE = 60.seconds

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)
        val WINDOW_OPEN = Instant.fromEpochSeconds(500)
        val WINDOW_CLOSED = Instant.fromEpochSeconds(1_500)
    }
}

/** A denomination costs what it is worth, so an account's balance reads as the denominations it can pay for. */
private object PowerOfTwoPricing : CoinageBalanceConversionContext {
    override fun formatExponentToBalance(exponent: ValueExponent): Balance =
        BigInteger.TWO.pow(exponent.value).intoBalance()

    override fun formatExponentToAmount(exponent: ValueExponent): BigDecimal =
        BigDecimal(formatExponentToBalance(exponent).value)
}
