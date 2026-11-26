package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldings
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.data.source.ClockChangesSource
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinRecyclingEvaluator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.FakeCoinagePrivacyConfigProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclingParamsProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclingStrategyProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RingCapacityProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.UnloadQuotaTracker
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherReadinessUpdates
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherUsabilityContextFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.forcedAgeOf
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class RealCoinageHoldingsUseCaseTest {
    @Test
    fun `a voucher moves from gaining privacy to usable at its anonymity deadline`() = runTest {
        val fixture = Fixture(this)
        val holdings = mutableListOf<CoinageHoldings>()
        backgroundScope.launch { fixture.useCase.subscribeHoldings().collect { holdings += it } }
        runCurrent()
        advanceTimeBy(599999)
        runCurrent()
        assertEquals(listOf(fixture.vouchers()), holdings.map { it.vouchers.gainingPrivacy })

        advanceTimeBy(1)
        runCurrent()

        assertEquals(listOf(emptyList(), fixture.vouchers()), holdings.map { it.vouchers.usable })
    }

    private class Fixture(scope: TestScope) {
        private val vouchers = MutableStateFlow(listOf(voucher()))
        private val assets: CoinageAssetsUseCase = mockk()
        private val settings: CoinageRecyclingStrategySettings = mockk()
        private val evaluator: CoinRecyclingEvaluator = mockk()
        private val capacities: RingCapacityProvider = mockk()
        private val quota: UnloadQuotaTracker = mockk()
        private val clockChanges: ClockChangesSource = mockk()
        private val clock = object : TimeProvider {
            override fun now() = Instant.fromEpochMilliseconds(scope.testScheduler.currentTime)
        }

        val useCase: RealCoinageHoldingsUseCase

        init {
            every { assets.subscribeCoins() } returns MutableStateFlow(emptyList<TrackedCoin>())
            every { assets.subscribeVouchers() } returns vouchers
            every { settings.strategyFlow() } returns flowOf(RecyclingStrategyType.BALANCED)
            every { evaluator.verdicts } returns flowOf(emptyMap())
            every { clockChanges.changes() } returns emptyFlow()
            coEvery { capacities.peekCapacitiesFor(any()) } returns mapOf(DENOMINATION to FULL_RING)

            val privacyConfig = FakeCoinagePrivacyConfigProvider()
            useCase = RealCoinageHoldingsUseCase(
                assets,
                RecyclingStrategyProvider(forcedAgeOf(FORCED_AGE), quota),
                RecyclingParamsProvider(settings, privacyConfig),
                evaluator,
                VoucherUsabilityContextFactory(capacities, clock),
                VoucherReadinessUpdates(clock, clockChanges),
            )
        }

        fun vouchers() = vouchers.value.map { it.voucher }
    }

    private companion object {
        const val FORCED_AGE = 14
        const val FULL_RING = 767
        const val RING_MEMBERS = 8
        val DENOMINATION = ValueExponent(1)

        fun voucher() = TrackedVoucher(
            RecyclerVoucher(
                ringVrfKeyIndex = testKey(0),
                ringVrfPublicKey = mockk(),
                recyclerValue = DENOMINATION,
                location = RecyclerVoucher.Location.InRecycler(
                    recyclerIndex = RecyclerIndex(BigInteger.ONE),
                    recyclerMembers = RING_MEMBERS,
                    enteredAt = Instant.fromEpochMilliseconds(0),
                    recyclerUnloaded = 0,
                ),
                recyclerFungibility = RecyclerFungibility.NONE,
                maxRecyclerFungibility = null,
            ),
            CoinageAssetState.UNTRACKED,
        )
    }
}
