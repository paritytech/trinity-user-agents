package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.TopUpRepository
import io.paritytech.polkadotapp.feature_products_impl.data.storage.TopUpSourceStorage
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.testDispatchers
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Registering a top-up and following it afterwards.
 *
 * The two are separate because a top-up outlives the call that asked for it, and nothing keeps a record of
 * one beyond what the ledger already holds: the operation is written into its coinage group's id, and the
 * only thing kept alongside is the source, for exactly as long as another attempt could still be made.
 */
@OptIn(ExperimentalTime::class)
class RealTopUpServiceTest {
    private val repository = InMemoryTopUpRepository()
    private val sourceStorage = InMemoryTopUpSourceStorage()
    private val executeTopUpUseCase = FakeExecuteTopUpUseCase()
    private val sourceResolver: TopUpSourceResolver = mock()
    private val acknowledgements: TopUpAcknowledgementPresenter = mock()
    private val timeProvider = object : TimeProvider {
        override fun now(): Instant = NOW
    }

    // ---- registering ----

    @Test
    fun `a top-up holds its source before it runs`() = runTest {
        withResolvableSource()

        assertTrue(service().start(PRODUCT, ID, REQUESTED, SOURCE).isSuccess)

        assertEquals(SOURCE, heldSource())
    }

    /**
     * An id names one operation and only one. The second call says nothing about whether its amount or
     * source match the first, and honouring it would be paying twice for one id.
     */
    @Test
    fun `an id whose source is still held is refused`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        val result = service.start(PRODUCT, ID, REQUESTED, SOURCE)

        assertTrue(result.exceptionOrNull() is TopUpError.AlreadyExists)
        assertEquals(1, executeTopUpUseCase.runs)
    }

    /**
     * The source of a finished top-up is thrown away, but its group stays in the ledger — and that is what
     * keeps the id spent. Otherwise an id could be reused the moment its top-up ended.
     */
    @Test
    fun `an id the ledger already holds a group for is refused`() = runTest {
        withResolvableSource()
        givenSettled(TopUpStatus.NotClaimed)

        val result = service().start(PRODUCT, ID, REQUESTED, SOURCE)

        assertTrue(result.exceptionOrNull() is TopUpError.AlreadyExists)
    }

    /** The ids are strings products pick for themselves, so one product's cannot collide with another's. */
    @Test
    fun `the same id from a different product is its own top-up`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, coins(COIN_A))
        advanceUntilIdle()

        val result = service.start(OTHER_PRODUCT, ID, REQUESTED, coins(COIN_B))

        assertTrue("expected success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    /** Nothing is held for a source that cannot be resolved: there would be no way to run it. */
    @Test
    fun `a source that cannot be resolved is refused and nothing is held`() = runTest {
        withUnresolvableSource()

        val result = service().start(PRODUCT, ID, REQUESTED, SOURCE)

        assertTrue(result.exceptionOrNull() is TopUpError.InvalidSource)
        assertNull(heldSource())
        assertEquals(0, executeTopUpUseCase.runs)
    }

    /**
     * Two claims of one coin race, and the chain refuses the loser. The source frees up as soon as the
     * top-up holding it reaches a verdict, so this is a wait rather than a permanent block.
     */
    @Test
    fun `a source another top-up is still claiming is refused`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, coins(COIN_A))
        advanceUntilIdle()

        val result = service.start(PRODUCT, topUpId("topup-2"), REQUESTED, coins(COIN_A))

        assertTrue("expected SourceBusy but was ${result.exceptionOrNull()}", result.exceptionOrNull() is TopUpError.SourceBusy)
    }

    /** A coin in common is enough: the keys are the money, not a way of reaching it. */
    @Test
    fun `a source overlapping another top-up's coins is refused`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, coins(COIN_A, COIN_B))
        advanceUntilIdle()

        val result = service.start(PRODUCT, topUpId("topup-2"), REQUESTED, coins(COIN_B))

        assertTrue(result.exceptionOrNull() is TopUpError.SourceBusy)
    }

    @Test
    fun `a source no other top-up is claiming is accepted`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, coins(COIN_A))
        advanceUntilIdle()

        val result = service.start(PRODUCT, topUpId("topup-2"), REQUESTED, coins(COIN_B))

        assertTrue("expected success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    /** The block lasts exactly as long as the claim does — a verdict lets go of the money. */
    @Test
    fun `a source is free again once the top-up holding it reaches a verdict`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.NotClaimed)
        val service = service()

        service.start(PRODUCT, ID, REQUESTED, coins(COIN_A))
        advanceUntilIdle()

        val result = service.start(PRODUCT, topUpId("topup-2"), REQUESTED, coins(COIN_A))

        assertTrue("expected success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    /** A derivation index indexes one product's subtree, so the same number is a different account. */
    @Test
    fun `the same product account index under a different product is a different source`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, PRODUCT_ACCOUNT)
        advanceUntilIdle()

        val result = service.start(OTHER_PRODUCT, topUpId("topup-2"), REQUESTED, PRODUCT_ACCOUNT)

        assertTrue("expected success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    @Test
    fun `the same product account index under the same product is refused`() = runTest {
        withResolvableSource()
        val service = service()
        service.start(PRODUCT, ID, REQUESTED, PRODUCT_ACCOUNT)
        advanceUntilIdle()

        val result = service.start(PRODUCT, topUpId("topup-2"), REQUESTED, PRODUCT_ACCOUNT)

        assertTrue(result.exceptionOrNull() is TopUpError.SourceBusy)
    }

    // ---- following ----

    @Test
    fun `an id that was never registered is not found`() = runTest {
        val error = runCatching { service().status(PRODUCT, ID).first() }.exceptionOrNull()

        assertTrue("expected NotFound but was $error", error is TopUpError.NotFound)
    }

    /** The ordinary case after a relaunch: nothing is running, and the held source is what makes it possible. */
    @Test
    fun `following a top-up this process never started picks it back up`() = runTest {
        withResolvableSource()
        givenUnfinished(SOURCE)
        executeTopUpUseCase.emits(TopUpStatus.Claimed(finalized = true))

        val status = service().status(PRODUCT, ID).first { it is TopUpStatus.Claimed }

        assertEquals(TopUpStatus.Claimed(finalized = true), status)
        assertEquals(1, executeTopUpUseCase.runs)
    }

    /**
     * Two followers must share one run. A second run against the same money would submit the same
     * transactions twice, which is the one thing the whole design is arranged to prevent.
     */
    @Test
    fun `two followers share the run rather than starting a second`() = runTest {
        withResolvableSource()
        givenUnfinished(SOURCE)
        executeTopUpUseCase.emits(TopUpStatus.Claiming)
        val service = service()

        service.status(PRODUCT, ID).first()
        service.status(PRODUCT, ID).first()
        advanceUntilIdle()

        assertEquals(1, executeTopUpUseCase.runs)
    }

    /** The product is not waiting on the call, so the top-up has to keep going with nobody listening. */
    @Test
    fun `a registered top-up runs without anyone following it`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.Claimed(finalized = true))

        service().start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        assertEquals(1, executeTopUpUseCase.runs)
    }

    /**
     * A verdict reached by a previous process, asked after by this one.
     *
     * Nothing is running, so the answer can only come from the record — and it must come from there rather
     * than be worked out again from transactions a fork can still move, which is what makes a terminal
     * status terminal.
     */
    @Test
    fun `a verdict from a previous run is reported without re-deriving it`() = runTest {
        givenSettled(TopUpStatus.ClaimedPartially(80.intoBalance()))
        executeTopUpUseCase.reportsFromLedger(TopUpStatus.NotClaimed)

        val status = service().status(PRODUCT, ID).first()

        assertEquals(TopUpStatus.ClaimedPartially(80.intoBalance()), status)
        assertEquals(0, executeTopUpUseCase.runs)
    }

    /**
     * An unfinished top-up whose source is gone cannot be attempted again — that is the one thing a retry
     * needs. What its transactions came to is still on the ledger, and is the honest answer.
     */
    @Test
    fun `an unfinished top-up whose source is gone reports what the ledger says`() = runTest {
        repository.insert(operation())
        executeTopUpUseCase.reportsFromLedger(TopUpStatus.ClaimedPartially(80.intoBalance()))

        val status = service().status(PRODUCT, ID).first()

        assertEquals(TopUpStatus.ClaimedPartially(80.intoBalance()), status)
        assertEquals(0, executeTopUpUseCase.runs)
    }

    /**
     * The ordinary failure: the money never arrived, so nothing was ever registered and the ledger has no
     * group to remember it by. The verdict still has to outlive the process, or a product that asks after
     * its own top-up is told the id does not exist.
     */
    @Test
    fun `a top-up that registered nothing still reports its verdict after settling`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.NotClaimed)
        val service = service()

        service.start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        assertEquals(TopUpStatus.NotClaimed, service.status(PRODUCT, ID).first())
    }

    /** And it keeps the id spent: an id is used once, whatever the top-up under it came to. */
    @Test
    fun `an id whose top-up registered nothing is never handed out again`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.NotClaimed)
        val service = service()

        service.start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        val result = service.start(PRODUCT, ID, REQUESTED, SOURCE)

        assertTrue("expected AlreadyExists but was ${result.exceptionOrNull()}", result.exceptionOrNull() is TopUpError.AlreadyExists)
    }

    /**
     * A verdict is what was decided, not what the entries happen to say now. Inclusion is not finality, so
     * re-deriving one would let a status a product has already been given change under it.
     */
    @Test
    fun `a verdict is reported as it was decided, not re-derived`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.ClaimedPartially(80.intoBalance()))
        val service = service()

        service.start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        // The ledger now says something else entirely; the verdict must not follow it.
        executeTopUpUseCase.reportsFromLedger(TopUpStatus.NotClaimed)

        assertEquals(TopUpStatus.ClaimedPartially(80.intoBalance()), service.status(PRODUCT, ID).first())
    }

    // ---- what is kept, and for how long ----

    /**
     * Two of the three sources are bearer secrets a product handed over, and they can only ever build
     * another attempt. Once there will not be another one they are no use to anybody but an attacker.
     */
    @Test
    fun `the source is dropped once the top-up reaches a verdict`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.Claimed(finalized = true))

        service().start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        assertNull("the source outlived the top-up that needed it", heldSource())
    }

    /**
     * An inclusion is not a verdict. Dropping the source there would throw the keys away while a fork could
     * still take the money back, leaving nothing able to claim it again.
     */
    @Test
    fun `an unfinalized claim keeps the source`() = runTest {
        withResolvableSource()
        executeTopUpUseCase.emits(TopUpStatus.Claimed(finalized = false))

        service().start(PRODUCT, ID, REQUESTED, SOURCE)
        advanceUntilIdle()

        assertEquals(SOURCE, heldSource())
    }

    // ---- resuming at launch ----

    /**
     * Waiting for a product to ask would leave money on a key nobody is watching for as long as that product
     * goes unopened — and the window would expire meanwhile.
     */
    @Test
    fun `every unfinished top-up is picked up at launch`() = runTest {
        withResolvableSource()
        givenUnfinished(SOURCE)
        executeTopUpUseCase.emits(TopUpStatus.Claiming)

        service().resumeUnfinished()
        advanceUntilIdle()

        assertEquals(1, executeTopUpUseCase.runs)
    }

    /** A held source is exactly an unfinished top-up, so a finished one is not picked up again. */
    @Test
    fun `a top-up that already reached a verdict is not picked up again`() = runTest {
        withResolvableSource()
        givenSettled(TopUpStatus.NotClaimed)

        service().resumeUnfinished()
        advanceUntilIdle()

        assertEquals(0, executeTopUpUseCase.runs)
    }

    /** A launch resume and a product asking after the same top-up must not both run it. */
    @Test
    fun `a launch resume does not start a second run of a top-up already going`() = runTest {
        withResolvableSource()
        givenUnfinished(SOURCE)
        executeTopUpUseCase.emits(TopUpStatus.Claiming)
        val service = service()

        service.status(PRODUCT, ID).first()
        service.resumeUnfinished()
        advanceUntilIdle()

        assertEquals(1, executeTopUpUseCase.runs)
    }

    /**
     * A source that stopped resolving is permanent, and every launch reaches it before the top-ups recorded
     * after it. Letting it end the sweep would leave those unresumed for as long as the bad one is kept.
     */
    @Test
    fun `a top-up that cannot be resumed does not stop the rest being picked up`() = runTest {
        withUnresolvableSource()
        withResolvableSource(SECOND_SOURCE)
        givenUnfinished(SOURCE)
        givenUnfinished(SECOND_SOURCE, SECOND_ID)
        executeTopUpUseCase.emits(TopUpStatus.Claiming)

        service().resumeUnfinished()
        advanceUntilIdle()

        assertEquals(1, executeTopUpUseCase.runs)
    }

    // ---- harness ----

    private fun TestScope.service(): TopUpService = RealTopUpService(
        repository = repository,
        sourceStorage = sourceStorage,
        executeTopUpUseCase = executeTopUpUseCase,
        sourceResolver = sourceResolver,
        acknowledgements = acknowledgements,
        timeProvider = timeProvider,
        dispatchers = testDispatchers(),
    )

    private suspend fun withResolvableSource() {
        val resolved = TopUpSource.Onboard(TransactionSignerSource.FromAccount(mock<MetaAccount>()))

        whenever(sourceResolver.resolve(any(), any())).thenReturn(Result.success(resolved))
    }

    private suspend fun withResolvableSource(source: PaymentTopUpSource) {
        val resolved = TopUpSource.Onboard(TransactionSignerSource.FromAccount(mock<MetaAccount>()))

        whenever(sourceResolver.resolve(any(), eq(source))).thenReturn(Result.success(resolved))
    }

    private suspend fun withUnresolvableSource() {
        whenever(sourceResolver.resolve(any(), any()))
            .thenReturn(Result.failure(IllegalStateException("no such account")))
    }

    /** A top-up a previous run registered and left unfinished, with its source still held. */
    private suspend fun givenUnfinished(source: PaymentTopUpSource, id: PaymentTopUpId = ID) {
        val operation = operation(id)
        repository.insert(operation)
        sourceStorage.put(operation.groupId, source)
    }

    /** The source still held for the top-up under test, if any. */
    private suspend fun heldSource(): PaymentTopUpSource? =
        repository.get(PRODUCT, ID)?.let { sourceStorage.get(it.groupId) }

    /** A top-up a previous run finished: its verdict is recorded and its source is long gone. */
    private suspend fun givenSettled(outcome: TopUpStatus) {
        val operation = operation()
        repository.insert(operation)
        repository.settle(operation, outcome)
    }

    private fun operation(id: PaymentTopUpId = ID) = TopUpOperation(id, PRODUCT, REQUESTED, NOW, outcome = null)

    private fun coins(vararg keys: DataByteArray) = PaymentTopUpSource.Coins(keys.toList())

    private companion object {
        val DOT_TLD: DotNsTld = requireNotNull(DotNsTld.parse("dot"))

        val ID = topUpId("topup-1")
        val SECOND_ID = topUpId("topup-2")
        val PRODUCT: ProductId = ProductId.fromString("alice.dot", DOT_TLD).getOrThrow()
        val OTHER_PRODUCT: ProductId = ProductId.fromString("bob.dot", DOT_TLD).getOrThrow()
        val REQUESTED: Balance = 100.intoBalance()
        val COIN_A: DataByteArray = byteArrayOf(1).toDataByteArray()
        val COIN_B: DataByteArray = byteArrayOf(2).toDataByteArray()

        val SOURCE = PaymentTopUpSource.Coins(listOf(COIN_A))
        val SECOND_SOURCE = PaymentTopUpSource.Coins(listOf(COIN_B))
        val PRODUCT_ACCOUNT = PaymentTopUpSource.ProductAccount(DerivationIndex32.fromUInt(0u))
        val NOW: Instant = Instant.fromEpochSeconds(1_000_000)
    }
}

/** A fake because [ExecuteTopUpUseCase.execute] takes value classes, which Mockito unwraps. */
private class FakeExecuteTopUpUseCase : ExecuteTopUpUseCase {
    private var statuses: List<TopUpStatus> = emptyList()
    private var verdict: TopUpStatus = TopUpStatus.NotClaimed

    var runs = 0
        private set

    fun emits(vararg statuses: TopUpStatus) {
        this.statuses = statuses.toList()
    }

    fun reportsFromLedger(verdict: TopUpStatus) {
        this.verdict = verdict
    }

    override fun execute(operation: TopUpOperation, source: TopUpSource): Flow<TopUpStatus> {
        runs++

        return flow { statuses.forEach { emit(it) } }
    }

    override suspend fun statusOf(operation: TopUpOperation): TopUpStatus = verdict
}

@OptIn(ExperimentalTime::class)
private class InMemoryTopUpRepository : TopUpRepository {
    private val operations = mutableMapOf<String, TopUpOperation>()

    override suspend fun insert(operation: TopUpOperation): Result<Unit> {
        val key = key(operation.productId, operation.id)
        if (operations.containsKey(key)) return Result.failure(IllegalStateException("id taken"))

        operations[key] = operation

        return Result.success(Unit)
    }

    override suspend fun get(productId: ProductId, id: PaymentTopUpId): TopUpOperation? = operations[key(productId, id)]

    override suspend fun unfinished(): List<TopUpOperation> = operations.values.filter { it.outcome == null }

    override suspend fun settle(operation: TopUpOperation, outcome: TopUpStatus): Result<Unit> {
        operations[key(operation.productId, operation.id)] = operation.copy(outcome = outcome)

        return Result.success(Unit)
    }

    suspend fun unfinishedIds() = unfinished().map { it.groupId }

    private fun key(productId: ProductId, id: PaymentTopUpId) = "${productId.value}:${id.asHex()}"
}

private class InMemoryTopUpSourceStorage : TopUpSourceStorage {
    private val sources = mutableMapOf<String, PaymentTopUpSource>()

    override suspend fun get(groupId: CoinageOperationGroupId): PaymentTopUpSource? = sources[groupId.value]

    override suspend fun put(groupId: CoinageOperationGroupId, source: PaymentTopUpSource): Result<Unit> {
        sources[groupId.value] = source

        return Result.success(Unit)
    }

    override suspend fun remove(groupId: CoinageOperationGroupId) {
        sources.remove(groupId.value)
    }
}
