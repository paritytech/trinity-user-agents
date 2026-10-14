package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

private const val FULL_RING = 767
private const val FORCED_AGE = 14

class CoinageAssetSelectorTest {
    private val coinageAssetsUseCase: CoinageAssetsUseCase = mockk()
    private val ringCapacityProvider: RingCapacityProvider = mockk()
    private val settings: CoinageRecyclingStrategySettings = mockk()
    private val evaluator: CoinRecyclingEvaluator = mockk()
    private val quotaTracker: UnloadQuotaTracker = mockk()
    private val coinRepository: CoinRepository = mockk()

    private var now = Instant.fromEpochMilliseconds(0)
    private val timeProvider = object : TimeProvider {
        override fun now(): Instant = now
    }

    private val selector = CoinageAssetSelector(
        coinageAssetsUseCase = coinageAssetsUseCase,
        strategyProvider = RecyclingStrategyProvider(ForcedRecyclingAgeProvider(coinRepository), quotaTracker),
        settings = settings,
        evaluator = evaluator,
        voucherUsabilityContextFactory = VoucherUsabilityContextFactory(ringCapacityProvider, timeProvider),
    )

    private val spendable = coinOf(derivationIndex = 1)
    private val heldForPrivacy = coinOf(derivationIndex = 2)
    private val pastChainLimit = coinOf(derivationIndex = 3)

    init {
        coEvery { coinRepository.getCoinRecyclingAge() } returns Result.success(FORCED_AGE)

        // The selector always asks for a COMPLETE context, so the fetched capacities are what it sees.
        coEvery { ringCapacityProvider.capacitiesFor(any()) } returns Result.success(mapOf(ValueExponent(1) to FULL_RING))
        coEvery { coinageAssetsUseCase.getCoins() } returns listOf(spendable, heldForPrivacy, pastChainLimit).map(::freeCoin)
        every { evaluator.verdicts } returns flowOf(
            mapOf(
                spendable.derivationIndex to CoinRecyclingState.ALLOW_USE,
                heldForPrivacy.derivationIndex to CoinRecyclingState.TO_RECYCLE,
                pastChainLimit.derivationIndex to CoinRecyclingState.MUST_RECYCLE,
            )
        )
    }

    /** Both answers come from one reading of the wallet, so a caller may take either without asking twice. */
    @Test
    fun `every scope is answered in a single pass`() = runBlocking<Unit> {
        withStrategy(RecyclingStrategyType.BALANCED)

        assertEquals(SpendScope.entries.toSet(), selector.getSelectableCoinsByScope().keys)
    }

    @Test
    fun `the plain scope takes only what is spendable outright`() = runBlocking<Unit> {
        withStrategy(RecyclingStrategyType.BALANCED)

        assertEquals(listOf(spendable), selector.getSelectableCoinsByScope().getValue(SpendScope.SPENDABLE))
    }

    @Test
    fun `a confirmed spend also takes what the strategy was holding back`() = runBlocking<Unit> {
        withStrategy(RecyclingStrategyType.BALANCED)

        assertEquals(
            listOf(spendable, heldForPrivacy),
            selector.getSelectableCoinsByScope().getValue(SpendScope.WITH_CONFIRMATION),
        )
    }

    /**
     * The confirmation cannot override the strategy. Max privacy refuses to make the offer at all, so asking
     * for the wider scope has to come back with the narrow answer rather than quietly spending its funds.
     */
    @Test
    fun `a strategy that refuses the offer is not widened by asking`() = runBlocking<Unit> {
        withStrategy(RecyclingStrategyType.MAX_PRIVACY)

        assertEquals(listOf(spendable), selector.getSelectableCoinsByScope().getValue(SpendScope.WITH_CONFIRMATION))
    }

    /** No confirmation makes a coin the chain rejects spendable, so it is absent from both answers. */
    @Test
    fun `a coin past the chain limit is never selectable`() = runBlocking<Unit> {
        withStrategy(RecyclingStrategyType.BALANCED)

        SpendScope.entries.forEach { scope ->
            val selected = selector.getSelectableCoins(scope)

            assertEquals("$scope offered a coin past the chain limit", false, pastChainLimit in selected)
        }
    }

    @Test
    fun `a confirmed spend also takes vouchers still gaining privacy`() = runBlocking<Unit> {
        val usable = voucherOf(ringVrfKeyIndex = 1, members = FULL_RING)
        val gaining = voucherOf(ringVrfKeyIndex = 2, members = 0)
        withVouchers(usable, gaining)
        withStrategy(RecyclingStrategyType.BALANCED)

        assertEquals(listOf(usable), selector.getSelectableVouchersByScope().getValue(SpendScope.SPENDABLE))
        assertEquals(
            listOf(usable, gaining),
            selector.getSelectableVouchersByScope().getValue(SpendScope.WITH_CONFIRMATION),
        )
    }

    @Test
    fun `a strategy that refuses the offer keeps its vouchers even for a confirmed spend`() = runBlocking<Unit> {
        val usable = voucherOf(ringVrfKeyIndex = 1, members = FULL_RING)
        val gaining = voucherOf(ringVrfKeyIndex = 2, members = 0)
        withVouchers(usable, gaining)
        withStrategy(RecyclingStrategyType.MAX_PRIVACY)

        assertEquals(listOf(usable), selector.getSelectableVouchersByScope().getValue(SpendScope.WITH_CONFIRMATION))
    }

    @Test
    fun `vouchers gaining privacy are reported apart from spendable ones`() = runBlocking<Unit> {
        val usable = voucherOf(ringVrfKeyIndex = 1, members = FULL_RING)
        val gaining = voucherOf(ringVrfKeyIndex = 2, members = 0)
        withVouchers(usable, gaining)
        withStrategy(RecyclingStrategyType.MAX_PRIVACY)

        assertEquals(listOf(gaining), selector.getVouchersGainingPrivacy())
    }

    @Test
    fun `maturity makes the voucher selectable without confirmation in both modes`() = runBlocking<Unit> {
        val voucher = voucherOf(ringVrfKeyIndex = 1, members = 32, enteredAt = now)
        withVouchers(voucher)
        val outcomes = listOf(RecyclingStrategyType.BALANCED, RecyclingStrategyType.MAX_PRIVACY).map { type ->
            withStrategy(type)
            now = Instant.fromEpochMilliseconds(0)
            val before = selector.getSelectableVouchersByScope()
            now += 10.minutes
            val after = selector.getSelectableVouchersByScope()
            before to after
        }

        val ready = SpendScope.entries.associateWith { listOf(voucher) }
        assertEquals(
            listOf(
                mapOf(SpendScope.SPENDABLE to emptyList(), SpendScope.WITH_CONFIRMATION to listOf(voucher)) to ready,
                SpendScope.entries.associateWith { emptyList<RecyclerVoucher>() } to ready,
            ),
            outcomes,
        )
    }

    private fun withStrategy(type: RecyclingStrategyType) {
        coEvery { settings.getStrategy() } returns type
    }

    private fun withVouchers(vararg vouchers: RecyclerVoucher) {
        coEvery { coinageAssetsUseCase.getVouchers() } returns vouchers.map { TrackedVoucher(it, CoinageAssetState.UNTRACKED) }
    }

    private fun freeCoin(coin: Coin) = TrackedCoin(coin, CoinageAssetState.UNTRACKED)

    private fun coinOf(derivationIndex: Int) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(3),
        isOnChain = true,
        accountId = mockk(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private fun voucherOf(ringVrfKeyIndex: Int, members: Int, enteredAt: Instant? = null) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(ringVrfKeyIndex),
        ringVrfPublicKey = mockk(),
        recyclerValue = ValueExponent(1),
        location = Location.InRecycler(RecyclerIndex(BigInteger.ONE), recyclerMembers = members, enteredAt = enteredAt),
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )
}
