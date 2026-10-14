package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.source.ClockChangesSource
import io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase.RealTotalBalanceUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import java.math.BigInteger
import kotlin.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class VoucherReadinessUpdatesTest {
    @Test
    fun `voucher-only wallet refreshes full balance while coin verdicts remain empty`() = runTest {
        val fixture = WalletFixture(this, RecyclingStrategyType.MAX_PRIVACY)
        val balances = mutableListOf<CoinageBalance>()
        val verdicts = mutableListOf<RecyclingVerdicts>()
        backgroundScope.launch { fixture.balance.subscribeTotalBalance().collect { balances += it.getOrThrow() } }
        backgroundScope.launch { fixture.evaluator.verdicts.collect { verdicts += it } }
        with(ComputationalScope(backgroundScope)) { fixture.evaluator.start() }
        runCurrent()
        advanceTimeBy(599999)
        runCurrent()
        assertEquals(listOf(fixture.balanceOf(ready = false)), balances)

        advanceTimeBy(1)
        runCurrent()

        assertEquals(listOf(fixture.balanceOf(ready = false), fixture.balanceOf(ready = true)), balances)
        assertEquals(listOf(emptyMap<CoinageKeyIndex, CoinRecyclingState>()), verdicts)
    }

    @Test
    fun `maturity frees recycling budget without an asset event`() = runTest {
        val fixture = WalletFixture(this, RecyclingStrategyType.BALANCED)
        val coin = Coin(testKey(0), ValueExponent(1), Coin.Age.Known(5), true, mock(), CoinProvenance.UNKNOWN)
        fixture.coins.value = listOf(TrackedCoin(coin, CoinageAssetState.UNTRACKED))
        val recycled = mutableListOf<List<Coin>>()
        whenever(fixture.recycling.recycle(any())).thenAnswer { invocation ->
            recycled += invocation.getArgument<List<Coin>>(0)
            Result.success(Unit)
        }
        val verdicts = mutableListOf<RecyclingVerdicts>()
        backgroundScope.launch { fixture.evaluator.verdicts.collect { verdicts += it } }
        with(ComputationalScope(backgroundScope)) { fixture.evaluator.start() }
        runCurrent()
        advanceTimeBy(599999)
        runCurrent()
        assertEquals(emptyList<List<Coin>>(), recycled)

        advanceTimeBy(1)
        runCurrent()

        assertEquals(listOf(listOf(coin)), recycled)
        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.last().getValue(testKey(0)))
    }

    @Test
    fun `removing a voucher cancels its old deadline and replacement gets its own`() = runTest {
        val fixture = SchedulerFixture(this)
        val emissions = mutableListOf<Long>()
        backgroundScope.launch {
            fixture.vouchers.withReadinessUpdates(fixture.updates, { it }, { RecyclingStrategyType.MAX_PRIVACY })
                .collect { emissions += testScheduler.currentTime }
        }
        runCurrent()
        advanceTimeBy(100000)
        fixture.vouchers.value = listOf(voucher(enteredAt = Instant.fromEpochMilliseconds(100000)))
        runCurrent()
        advanceTimeBy(500000)
        runCurrent()
        assertEquals(listOf(0L, 100000L), emissions)

        advanceTimeBy(100000)
        runCurrent()
        assertEquals(listOf(0L, 100000L, 700000L), emissions)
    }

    @Test
    fun `switching to minimum privacy cancels timeout and switching back keeps elapsed time`() = runTest {
        val fixture = SchedulerFixture(this)
        val emissions = mutableListOf<Long>()
        val inputs = combine(fixture.vouchers, fixture.strategy) { vouchers, strategy -> vouchers to strategy }
        backgroundScope.launch {
            inputs.withReadinessUpdates(fixture.updates, { it.first }, { it.second })
                .collect { emissions += testScheduler.currentTime }
        }
        runCurrent()
        advanceTimeBy(100000)
        fixture.strategy.value = RecyclingStrategyType.MIN_PRIVACY
        runCurrent()
        advanceTimeBy(500000)
        runCurrent()
        assertEquals(listOf(0L, 100000L), emissions)

        fixture.strategy.value = RecyclingStrategyType.MAX_PRIVACY
        runCurrent()
        assertEquals(listOf(0L, 100000L, 600000L), emissions)
    }

    @Test
    fun `backward clock adjustment postpones the wake until maturity`() = runTest {
        val fixture = SchedulerFixture(this)
        val emissions = mutableListOf<Long>()
        backgroundScope.launch {
            fixture.vouchers.withReadinessUpdates(fixture.updates, { it }, { RecyclingStrategyType.MAX_PRIVACY })
                .collect { emissions += testScheduler.currentTime }
        }
        runCurrent()
        fixture.clockOffset = -60000
        advanceTimeBy(600000)
        runCurrent()
        assertEquals(listOf(0L), emissions)

        advanceTimeBy(60000)
        runCurrent()
        assertEquals(listOf(0L, 660000L), emissions)
    }

    @Test
    fun `claiming a voucher removes its pending deadline`() = runTest {
        val fixture = SchedulerFixture(this)
        val emissions = mutableListOf<Long>()
        backgroundScope.launch {
            fixture.vouchers.withReadinessUpdates(fixture.updates, { it }, { RecyclingStrategyType.MAX_PRIVACY })
                .collect { emissions += testScheduler.currentTime }
        }
        runCurrent()
        advanceTimeBy(300000)
        fixture.vouchers.value = fixture.vouchers.value.map {
            it.copy(state = CoinageAssetState(handedOff = true, minterStatus = null, consumerStatus = null))
        }
        runCurrent()
        advanceTimeBy(600000)
        runCurrent()

        assertEquals(listOf(0L, 300000L), emissions)
    }

    @Test
    fun `a slow collector receives only the latest pending wallet snapshot`() = runTest {
        val fixture = SchedulerFixture(this)
        val inputs = MutableStateFlow(0)
        val releaseCollector = CompletableDeferred<Unit>()
        val received = mutableListOf<Int>()
        backgroundScope.launch {
            inputs.withReadinessUpdates(fixture.updates, { emptyList() }, { RecyclingStrategyType.MIN_PRIVACY })
                .collect { snapshot ->
                    received += snapshot
                    if (snapshot == 0) releaseCollector.await()
                }
        }
        runCurrent()
        inputs.value = 1
        runCurrent()
        inputs.value = 2
        runCurrent()
        releaseCollector.complete(Unit)
        runCurrent()

        assertEquals(listOf(0, 2), received)
    }

    @Test
    fun `clock jumps refresh balance and selection even after the previous deadline completed`() = runTest {
        val fixture = WalletFixture(this, RecyclingStrategyType.MAX_PRIVACY)
        val balances = mutableListOf<CoinageBalance>()
        val selected = mutableListOf<Map<SpendScope, List<RecyclerVoucher>>>()
        backgroundScope.launch { fixture.balance.subscribeTotalBalance().collect { balances += it.getOrThrow() } }
        with(ComputationalScope(backgroundScope)) { fixture.evaluator.start() }
        runCurrent()
        selected += fixture.selector.getSelectableVouchersByScope()

        fixture.clockOffset = 600000
        fixture.clockChanges.emit(Unit)
        runCurrent()
        selected += fixture.selector.getSelectableVouchersByScope()

        fixture.clockOffset = 0
        fixture.clockChanges.emit(Unit)
        runCurrent()
        selected += fixture.selector.getSelectableVouchersByScope()

        advanceTimeBy(600000)
        runCurrent()
        selected += fixture.selector.getSelectableVouchersByScope()

        val withheld = SpendScope.entries.associateWith { emptyList<RecyclerVoucher>() }
        val ready = SpendScope.entries.associateWith { fixture.vouchers.value.map { it.voucher } }
        assertEquals(listOf(withheld, ready, withheld, ready), selected)
        assertEquals(
            listOf(false, true, false, true).map { fixture.balanceOf(ready = it) },
            balances,
        )
    }

    private open class SchedulerFixture(scope: TestScope, type: RecyclingStrategyType = RecyclingStrategyType.MAX_PRIVACY) {
        val strategy = MutableStateFlow(type)
        val vouchers = MutableStateFlow(listOf(voucher()))
        var clockOffset = 0L
        val clockChanges = MutableSharedFlow<Unit>()
        val clock = object : TimeProvider {
            override fun now() = Instant.fromEpochMilliseconds(scope.testScheduler.currentTime + clockOffset)
        }
        val updates = VoucherReadinessUpdates(clock, mock<ClockChangesSource>().also {
            whenever(it.changes()).thenReturn(clockChanges)
        })
    }

    private class WalletFixture(scope: TestScope, type: RecyclingStrategyType) : SchedulerFixture(scope, type) {
        val coins = MutableStateFlow(emptyList<TrackedCoin>())
        val recycling: CoinageRecyclingUseCase = mock()
        private val assets: CoinageAssetsUseCase = mock()
        private val settings: CoinageRecyclingStrategySettings = mock()
        private val capacities: RingCapacityProvider = mock()
        private val converter: CoinageBalanceConverterUseCase = mock()
        private val quota: UnloadQuotaTracker = mock()
        private val provider = RecyclingStrategyProvider(forcedAgeOf(14), quota)
        private val contextFactory = VoucherUsabilityContextFactory(capacities, clock)
        private val dispatchers: CoroutineDispatchers = mock()

        init {
            runBlocking {
                whenever(assets.subscribeCoins()).thenReturn(coins)
                whenever(assets.subscribeVouchers()).thenReturn(vouchers)
                whenever(assets.getVouchers()).thenAnswer { vouchers.value }
                whenever(settings.getStrategy()).thenAnswer { strategy.value }
                whenever(settings.strategyFlow()).thenReturn(strategy)
                whenever(capacities.peekCapacitiesFor(any())).thenReturn(mapOf(ValueExponent(1) to 767))
                whenever(capacities.capacitiesFor(any())).thenReturn(Result.success(mapOf(ValueExponent(1) to 767)))
                whenever(converter.create()).thenReturn(Result.success(testConversionContext))
                whenever(quota.isQuotaRunningLow()).thenReturn(Result.success(false))
                whenever(dispatchers.computation).thenReturn(StandardTestDispatcher(scope.testScheduler))
            }
        }

        val evaluator = CoinRecyclingEvaluator(assets, settings, provider, contextFactory, converter, recycling, dispatchers, updates)
        val balance = RealTotalBalanceUseCase(assets, converter, provider, settings, evaluator, contextFactory, updates)
        val selector = CoinageAssetSelector(assets, provider, settings, evaluator, contextFactory)

        fun balanceOf(ready: Boolean): CoinageBalance {
            val amount = testConversionContext.formatExponentToBalance(ValueExponent(1))
            val zero = amount - amount
            return CoinageBalance(
                availablePrivate = if (ready) amount else zero,
                gainingPrivacy = CoinageBalance.GainingPrivacyBalance(if (ready) zero else amount, false),
                pending = zero,
            )
        }
    }

    private companion object {
        fun voucher(enteredAt: Instant = Instant.fromEpochMilliseconds(0)) = TrackedVoucher(
            RecyclerVoucher(
                ringVrfKeyIndex = testKey(0),
                ringVrfPublicKey = mock(),
                recyclerValue = ValueExponent(1),
                location = RecyclerVoucher.Location.InRecycler(RecyclerIndex(BigInteger.ONE), 32, enteredAt),
                recyclerFungibility = RecyclerFungibility.NONE,
                maxRecyclerFungibility = null,
            ),
            CoinageAssetState.UNTRACKED,
        )
    }
}
