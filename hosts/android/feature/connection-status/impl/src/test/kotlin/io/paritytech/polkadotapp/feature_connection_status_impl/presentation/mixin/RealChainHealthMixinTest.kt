package io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_connection_status_api.domain.ChainHealthMonitor
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthItemModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.IndicatorRow
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStorePeer
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.seconds

class RealChainHealthMixinTest {
    private val monitor: ChainHealthMonitor = mock(ChainHealthMonitor::class.java)
    private val peer: StatementStorePeer = mock(StatementStorePeer::class.java)
    private val answered = MutableStateFlow(true)
    private val lifecycle = FakeAppLifecycleObserver()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val subscriptions = AtomicInteger()
    private val cancellations = AtomicInteger()

    @Before
    fun setUp() {
        whenever(peer.chainId).thenReturn(PEOPLE)
        whenever(peer.observeAnswered()).thenReturn(answered)
    }

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun `items carry the row and indicator of their chain`() = runBlocking<Unit> {
        withMonitorEmitting(health(HUB))
        val mixin = createMixin()

        val item = mixin.awaitModel().chainRows().single()

        assertEquals(IndicatorRow.AssetHub, item.row)
        assertEquals(ChainHealthIndicator.Healthy(liveness = null), item.indicator)
    }

    @Test
    fun `the production share and the chain's block time shape the indicator`() = runBlocking<Unit> {
        withMonitorEmitting(health(HUB, blockProduction(produced = 8)))
        val mixin = createMixin()

        val item = mixin.awaitModel().chainRows().single()

        assertEquals(ChainHealthIndicator.of(share = 0.8f, expectedBlockTime = 6.seconds), item.indicator)
    }

    @Test
    fun `a chain not yet measured is healthy with nothing to print`() = runBlocking<Unit> {
        withMonitorEmitting(health(HUB, blockProduction(produced = null)))
        val mixin = createMixin()

        assertEquals(ChainHealthIndicator.Healthy(liveness = null), mixin.awaitModel().chainRows().single().indicator)
    }

    @Test
    fun `what a chain showed before stands while it is asked how fast it has been going`() = runBlocking<Unit> {
        val healths = withMonitorSequence()
        val mixin = createMixin()

        healths.emit(listOf(health(HUB, blockProduction(produced = 8))))
        mixin.awaitModel()
        healths.emit(listOf(health(HUB, anchorPending())))
        healths.emit(listOf(health(HUB, anchorPending()), health(PEOPLE, anchorPending())))

        val model = withTimeout(TIMEOUT) { mixin.model.first { it.chainRows().size == 2 } }

        assertEquals(ChainHealthIndicator.of(share = 0.8f, expectedBlockTime = 6.seconds), model.chainRows().first().indicator)
        assertEquals(ChainHealthIndicator.Healthy(liveness = null), model.chainRows().last().indicator)
    }

    @Test
    fun `a chain that was connecting is not held on its way back`() = runBlocking<Unit> {
        val healths = withMonitorSequence()
        val mixin = createMixin()

        healths.emit(listOf(health(HUB, connection = ChainConnectionPresentation.Connecting)))
        mixin.awaitModel()
        healths.emit(listOf(health(HUB, anchorPending())))

        val model = withTimeout(TIMEOUT) { mixin.model.first { it.chainRows().single().indicator != ChainHealthIndicator.Connecting } }

        assertEquals(ChainHealthIndicator.Healthy(liveness = null), model.chainRows().single().indicator)
    }

    @Test
    fun `a chain that lost its connection is never held`() = runBlocking<Unit> {
        val healths = withMonitorSequence()
        val mixin = createMixin()

        healths.emit(listOf(health(HUB, blockProduction(produced = 8))))
        mixin.awaitModel()
        healths.emit(listOf(health(HUB, anchorPending(), connection = ChainConnectionPresentation.Offline)))

        val model = withTimeout(TIMEOUT) { mixin.model.first { it.chainRows().single().indicator == ChainHealthIndicator.Offline } }

        assertEquals(ChainHealthIndicator.Offline, model.chainRows().single().indicator)
    }

    @Test
    fun `the statement store is drawn last, after every chain`() = runBlocking<Unit> {
        withMonitorEmitting(health(PEOPLE), health(HUB), health(BULLETIN))
        val mixin = createMixin()

        val model = mixin.awaitModel()

        assertEquals(4, model.rows.size)
        assertEquals(IndicatorRow.StatementStore, model.rows.last().row)
    }

    @Test
    fun `the statement store follows the chain whose socket it shares, not the others`() = runBlocking<Unit> {
        withMonitorEmitting(
            health(HUB, blockProduction(produced = 10)),
            health(PEOPLE, connection = ChainConnectionPresentation.Disconnected),
        )
        val mixin = createMixin()

        val model = mixin.awaitModel()

        assertEquals(ChainHealthIndicator.Disconnected, model.statementStoreRow().indicator)
    }

    @Test
    fun `a connected chain the peer has not answered leaves the statement store connecting`() = runBlocking<Unit> {
        answered.value = false
        withMonitorEmitting(health(PEOPLE, blockProduction(produced = 10)))
        val mixin = createMixin()

        val model = mixin.awaitModel()

        assertEquals(ChainHealthIndicator.of(share = 1f, expectedBlockTime = 6.seconds), model.chainRows().single().indicator)
        assertEquals(ChainHealthIndicator.Connecting, model.statementStoreRow().indicator)
    }

    @Test
    fun `the statement store never carries the chain's block production`() = runBlocking<Unit> {
        withMonitorEmitting(health(PEOPLE, blockProduction(produced = 4)))
        val mixin = createMixin()

        val model = mixin.awaitModel()

        assertEquals(ChainHealthIndicator.of(share = 0.4f, expectedBlockTime = 6.seconds), model.chainRows().single().indicator)
        assertEquals(ChainHealthIndicator.Healthy(liveness = null), model.statementStoreRow().indicator)
    }

    @Test
    fun `the monitor is observed only while the app is in the foreground`() = runBlocking<Unit> {
        withMonitorEmitting(health(PEOPLE))
        val mixin = createMixin()
        val foregroundModel = mixin.awaitModel()
        assertEquals(1, subscriptions.get())

        lifecycle.state.value = AppLifecycleState.BACKGROUND
        awaitUntil { cancellations.get() == 1 }
        assertEquals(foregroundModel, mixin.model.value)

        lifecycle.state.value = AppLifecycleState.FOREGROUND
        awaitUntil { subscriptions.get() == 2 }
    }

    private fun createMixin() = RealChainHealthMixin(
        scope = ComputationalScope(scope),
        monitor = monitor,
        peer = peer,
        knownChains = KnownChains(people = PEOPLE, assetHub = HUB, bulletIn = BULLETIN, hydration = null),
        appLifecycleObserver = lifecycle,
    )

    private fun ChainHealthIndicatorsModel.chainRows(): List<ChainHealthItemModel> =
        rows.filterNot { it.row == IndicatorRow.StatementStore }

    private fun ChainHealthIndicatorsModel.statementStoreRow(): ChainHealthItemModel =
        rows.single { it.row == IndicatorRow.StatementStore }

    private fun withMonitorSequence(): MutableSharedFlow<List<ChainHealth>> {
        val healths = MutableSharedFlow<List<ChainHealth>>(replay = 1)
        whenever(monitor.observeChainsHealth()).thenReturn(healths)
        return healths
    }

    private fun withMonitorEmitting(vararg healths: ChainHealth) {
        val observed = flow {
            subscriptions.incrementAndGet()
            emit(healths.toList())
            awaitCancellation()
        }.onCompletion { cancellations.incrementAndGet() }
        whenever(monitor.observeChainsHealth()).thenReturn(observed)
    }

    private suspend fun RealChainHealthMixin.awaitModel() =
        withTimeout(TIMEOUT) { model.first { it.chainRows().isNotEmpty() } }

    private suspend fun awaitUntil(condition: () -> Boolean) = withTimeout(TIMEOUT) {
        while (!condition()) delay(POLL_INTERVAL_MS)
    }

    private fun health(
        chainId: String,
        vararg readings: ChainMetricReading,
        connection: ChainConnectionPresentation = ChainConnectionPresentation.Connected,
    ) = ChainHealth(
        chainId = chainId,
        chainName = chainId,
        connection = connection,
        expectedBlockTime = 6.seconds,
        readings = readings.toList(),
    )

    private fun blockProduction(produced: Int?) = ChainMetricReading.BlockProduction(
        producedBlocks = produced,
        expectedBlocks = 10,
        anchorPending = false,
    )

    private fun anchorPending() = ChainMetricReading.BlockProduction(
        producedBlocks = null,
        expectedBlocks = 10,
        anchorPending = true,
    )

    private class FakeAppLifecycleObserver : AppLifecycleObserver {
        val state = MutableStateFlow(AppLifecycleState.FOREGROUND)

        override fun subscribe(): Flow<AppLifecycleState> = state

        override fun getCurrentState(): AppLifecycleState = state.value
    }

    private companion object {
        const val PEOPLE = "people"
        const val HUB = "hub"
        const val BULLETIN = "bulletin"
        const val POLL_INTERVAL_MS = 10L
        val TIMEOUT = 5.seconds
    }
}
