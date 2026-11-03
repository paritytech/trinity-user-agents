package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
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
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class CoinRecyclingEvaluatorTest {
    @Test
    fun `verdicts keep updating when a recycle is still waiting`() = runTest {
        val fixture = EvaluatorFixture(this, recycleRelease = CompletableDeferred())

        fixture.start()
        fixture.addSecondDueCoin()

        assertTrue(SECOND_DUE_COIN.derivationIndex in fixture.verdicts.last())
    }

    @Test
    fun `no second recycle starts when the first one is still running`() = runTest {
        val fixture = EvaluatorFixture(this, recycleRelease = CompletableDeferred())

        fixture.start()
        fixture.addSecondDueCoin()

        assertEquals(listOf(listOf(DUE_COIN)), fixture.recycled)
    }

    @Test
    fun `the newest evaluation is recycled when the running attempt finishes`() = runTest {
        val recycleRelease = CompletableDeferred<Unit>()
        val fixture = EvaluatorFixture(this, recycleRelease)

        fixture.start()
        fixture.addSecondDueCoin()
        recycleRelease.complete(Unit)
        runCurrent()

        assertEquals(listOf(listOf(DUE_COIN), listOf(DUE_COIN, SECOND_DUE_COIN)), fixture.recycled)
    }

    @Test
    fun `the next evaluation recycles again when the previous attempt has finished`() = runTest {
        val fixture = EvaluatorFixture(this, recycleRelease = CompletableDeferred(Unit))

        fixture.start()
        fixture.addSecondDueCoin()

        assertEquals(listOf(listOf(DUE_COIN), listOf(DUE_COIN, SECOND_DUE_COIN)), fixture.recycled)
    }

    private class EvaluatorFixture(private val scope: TestScope, recycleRelease: CompletableDeferred<Unit>) {
        val recycled = mutableListOf<List<Coin>>()
        val verdicts = mutableListOf<RecyclingVerdicts>()

        private val coins = MutableStateFlow(listOf(tracked(DUE_COIN)))
        private val clock = FakeTimeProvider { scope.testScheduler.currentTime }

        private val assets = mockk<CoinageAssetsUseCase> {
            every { subscribeCoins() } returns coins
            every { subscribeVouchers() } returns MutableStateFlow(emptyList<TrackedVoucher>())
        }
        private val settings = mockk<CoinageRecyclingStrategySettings> {
            every { strategyFlow() } returns MutableStateFlow(RecyclingStrategyType.MIN_PRIVACY)
        }
        private val capacities = mockk<RingCapacityProvider> {
            coEvery { peekCapacitiesFor(any()) } returns emptyMap()
            coEvery { capacitiesFor(any()) } returns Result.success(emptyMap())
        }
        private val converter = mockk<CoinageBalanceConverterUseCase> {
            coEvery { create() } returns Result.success(testConversionContext)
        }
        private val quota = mockk<UnloadQuotaTracker> {
            coEvery { isQuotaRunningLow() } returns Result.success(false)
        }
        private val recycling = mockk<CoinageRecyclingUseCase> {
            coEvery { recycle(any()) } coAnswers {
                recycled += firstArg<List<Coin>>()
                recycleRelease.await()
                Result.success(Unit)
            }
        }
        private val clockChanges = mockk<ClockChangesSource> {
            every { changes() } returns emptyFlow()
        }

        private val evaluator = CoinRecyclingEvaluator(
            assets,
            settings,
            RecyclingStrategyProvider(forcedAgeOf(FORCED_AGE), quota),
            VoucherUsabilityContextFactory(capacities, clock),
            converter,
            recycling,
            scope.testDispatchers(),
            VoucherReadinessUpdates(clock, clockChanges),
        )

        fun start() {
            scope.backgroundScope.launch { evaluator.verdicts.collect { verdicts += it } }
            with(ComputationalScope(scope.backgroundScope)) { evaluator.start() }
            scope.runCurrent()
        }

        fun addSecondDueCoin() {
            coins.value = listOf(tracked(DUE_COIN), tracked(SECOND_DUE_COIN))
            scope.advanceTimeBy(PAST_EVALUATION_INTERVAL)
            scope.runCurrent()
        }
    }

    private companion object {
        const val FORCED_AGE = 14

        val PAST_EVALUATION_INTERVAL = 6.seconds

        val DUE_COIN = dueCoin(derivationIndex = 0)
        val SECOND_DUE_COIN = dueCoin(derivationIndex = 1)

        fun dueCoin(derivationIndex: Int) =
            Coin(testKey(derivationIndex), ValueExponent(1), Coin.Age.Known(FORCED_AGE), true, mockk(), CoinProvenance.UNKNOWN)

        fun tracked(coin: Coin) = TrackedCoin(coin, CoinageAssetState.UNTRACKED)
    }
}
