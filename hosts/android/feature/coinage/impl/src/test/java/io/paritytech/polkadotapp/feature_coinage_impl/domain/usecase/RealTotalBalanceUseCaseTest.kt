package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinRecyclingEvaluator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.ForcedRecyclingAgeProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclingStrategyProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RingCapacityProvider
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.UnloadQuotaTracker
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.VoucherUsabilityContextFactory
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.test_shared.any
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.`when`
import java.math.BigInteger
import kotlin.time.Instant

private const val FULL_RING = 767
private const val FORCED_AGE = 14

/**
 * The buckets, not the classification underneath — that lives in `CoinagePreClassificationTest`. What is
 * asserted here is which bucket each already-classified asset lands in, and that a coin the evaluator has
 * not judged yet is treated as still arriving rather than as spendable.
 */
class RealTotalBalanceUseCaseTest {
    private val coinRepository: CoinRepository = mock()
    private val coinageAssetsUseCase: CoinageAssetsUseCase = mock()
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase = mock()
    private val ringCapacityProvider: RingCapacityProvider = mock()
    private val settings: CoinageRecyclingStrategySettings = mock()
    private val evaluator: CoinRecyclingEvaluator = mock()
    private val quotaTracker: UnloadQuotaTracker = mock()

    private val timeProvider = object : TimeProvider {
        override fun now(): Instant = Instant.fromEpochMilliseconds(0)
    }

    private val strategyProvider = RecyclingStrategyProvider(ForcedRecyclingAgeProvider(coinRepository), quotaTracker)

    private val useCase: RealTotalBalanceUseCase

    init {
        runBlocking {
            `when`(coinageBalanceConverterUseCase.create()).thenReturn(Result.success(testConversionContext))

            // The balance asks for an IMMEDIATE context so it never waits on the chain, which makes the
            // cached capacities — not a fetch — what it classifies vouchers against.
            `when`(ringCapacityProvider.peekCapacitiesFor(any()))
                .thenReturn(mapOf(ValueExponent(1) to FULL_RING, ValueExponent(2) to FULL_RING))

            `when`(coinRepository.getCoinRecyclingAge()).thenReturn(Result.success(FORCED_AGE))
        }

        useCase = RealTotalBalanceUseCase(
            coinageAssetsUseCase = coinageAssetsUseCase,
            coinageBalanceConverterUseCase = coinageBalanceConverterUseCase,
            strategyProvider = strategyProvider,
            settings = settings,
            evaluator = evaluator,
            usabilityContextFactory = VoucherUsabilityContextFactory(ringCapacityProvider, timeProvider),
            readinessUpdates = mock(),
        )
    }

    @Test
    fun `empty data returns zero balance`() {
        assertBalance(coins = emptyList(), vouchers = emptyList(), expected = balanceOf())
    }

    @Test
    fun `a coin the strategy allows is available`() {
        val coin = coinOf(exponent = 1, age = 3)

        assertBalance(
            coins = listOf(coin),
            vouchers = emptyList(),
            verdicts = mapOf(coin.derivationIndex to CoinRecyclingState.ALLOW_USE),
            expected = balanceOf(spendable = 1.exponentToBalance()),
        )
    }

    @Test
    fun `a coin the strategy gated is gaining privacy, not spendable`() {
        val coin = coinOf(exponent = 1, age = 3)

        assertBalance(
            coins = listOf(coin),
            vouchers = emptyList(),
            verdicts = mapOf(coin.derivationIndex to CoinRecyclingState.TO_RECYCLE),
            expected = balanceOf(gainingPrivacy = 1.exponentToBalance()),
        )
    }

    /**
     * The correction that follows is upward. The other way round would put money on screen as spendable and
     * take it away a moment later, which is the one direction a balance must never move on its own.
     */
    @Test
    fun `a settled coin the evaluator has not judged yet counts as pending`() {
        val coin = coinOf(exponent = 1, age = 3)

        assertBalance(
            coins = listOf(coin),
            vouchers = emptyList(),
            verdicts = emptyMap(),
            expected = balanceOf(pending = 1.exponentToBalance()),
        )
    }

    @Test
    fun `a coin still arriving is pending`() {
        val coin = coinOf(exponent = 1, age = null, onChain = false)

        assertBalance(
            coins = listOf(coin),
            coinStates = listOf(stateWithMinter(DurableTxStatus.PENDING)),
            vouchers = emptyList(),
            expected = balanceOf(pending = 1.exponentToBalance()),
        )
    }

    /**
     * The regression: a transfer's change coin finalized while presence still read as absent, and the total
     * fell by the change instead of the amount sent. Total may lag into `available`, never out of existence.
     */
    @Test
    fun `a coin whose mint finalized before presence caught up stays pending`() {
        val coin = coinOf(exponent = 1, age = null, onChain = false)

        assertBalance(
            coins = listOf(coin),
            coinStates = listOf(stateWithMinter(DurableTxStatus.FINALIZED_SUCCESS)),
            vouchers = emptyList(),
            expected = balanceOf(pending = 1.exponentToBalance()),
        )
    }

    @Test
    fun `a coin whose mint failed counts nowhere`() {
        val coin = coinOf(exponent = 1, age = null, onChain = false)

        assertBalance(
            coins = listOf(coin),
            coinStates = listOf(stateWithMinter(DurableTxStatus.FAILURE)),
            vouchers = emptyList(),
            expected = balanceOf(),
        )
    }

    @Test
    fun `under min privacy an in-recycler voucher is available whatever its ring or delay`() {
        val voucher = voucherOf(exponent = 1, location = inRecycler(members = 0))

        assertBalance(
            coins = emptyList(),
            vouchers = listOf(voucher),
            strategyType = RecyclingStrategyType.MIN_PRIVACY,
            expected = balanceOf(spendable = 1.exponentToBalance()),
        )
    }

    @Test
    fun `under max privacy a voucher in a part-filled ring is gaining privacy`() {
        val voucher = voucherOf(
            exponent = 1,
            location = inRecycler(members = 690),
        )

        assertBalance(
            coins = emptyList(),
            vouchers = listOf(voucher),
            strategyType = RecyclingStrategyType.MAX_PRIVACY,
            expected = balanceOf(gainingPrivacy = 1.exponentToBalance(), canSpendWithConfirmation = false),
        )
    }

    @Test
    fun `under balanced twenty percent releases a voucher`() {
        val voucher = voucherOf(exponent = 1, location = inRecycler(members = 154))

        assertBalance(
            coins = emptyList(),
            vouchers = listOf(voucher),
            strategyType = RecyclingStrategyType.BALANCED,
            expected = balanceOf(spendable = 1.exponentToBalance()),
        )
    }

    @Test
    fun `under balanced an empty ring holds the voucher back`() {
        val voucher = voucherOf(exponent = 1, location = inRecycler(members = 0))

        assertBalance(
            coins = emptyList(),
            vouchers = listOf(voucher),
            strategyType = RecyclingStrategyType.BALANCED,
            expected = balanceOf(gainingPrivacy = 1.exponentToBalance()),
        )
    }

    @Test
    fun `an onboarding voucher is pending`() {
        val voucher = voucherOf(exponent = 1, location = Location.Onboarding)

        assertBalance(
            coins = emptyList(),
            vouchers = listOf(voucher),
            expected = balanceOf(pending = 1.exponentToBalance()),
        )
    }

    @Test
    fun `total sums every bucket`() {
        val available = coinOf(exponent = 1, age = 3, derivationIndex = 1)
        val gated = coinOf(exponent = 2, age = 9, derivationIndex = 2)
        val arriving = coinOf(exponent = 3, age = null, onChain = false, derivationIndex = 3)

        val balance = calculate(
            coins = listOf(available, gated, arriving),
            coinStates = listOf(
                CoinageAssetState.UNTRACKED,
                CoinageAssetState.UNTRACKED,
                stateWithMinter(DurableTxStatus.PENDING),
            ),
            vouchers = emptyList(),
            verdicts = mapOf(
                available.derivationIndex to CoinRecyclingState.ALLOW_USE,
                gated.derivationIndex to CoinRecyclingState.TO_RECYCLE,
            ),
        )

        assertEquals(listOf(1, 2, 3).exponentsToBalance(), balance.total)
        assertEquals(1.exponentToBalance(), balance.availablePrivate)
        assertEquals(2.exponentToBalance(), balance.gainingPrivacy.amount)
        assertEquals(3.exponentToBalance(), balance.pending)
    }

    /**
     * A coin the chain will not take is not a privacy trade the user could accept, so it must not appear
     * beside the balance they can choose to spend.
     */
    @Test
    fun `a coin past the forced age is pending, not gaining privacy`() {
        val forced = coinOf(exponent = 1, age = 3)

        assertBalance(
            coins = listOf(forced),
            vouchers = emptyList(),
            verdicts = mapOf(forced.derivationIndex to CoinRecyclingState.MUST_RECYCLE),
            expected = balanceOf(pending = 1.exponentToBalance()),
        )
    }

    @Test
    fun `max privacy will not offer what it is holding back`() {
        val gated = coinOf(exponent = 1, age = 3)

        assertBalance(
            coins = listOf(gated),
            vouchers = emptyList(),
            strategyType = RecyclingStrategyType.MAX_PRIVACY,
            verdicts = mapOf(gated.derivationIndex to CoinRecyclingState.TO_RECYCLE),
            expected = balanceOf(gainingPrivacy = 1.exponentToBalance(), canSpendWithConfirmation = false),
        )
    }

    @Test
    fun `balanced offers what it is holding back`() {
        val gated = coinOf(exponent = 1, age = 5)

        assertBalance(
            coins = listOf(gated),
            vouchers = emptyList(),
            strategyType = RecyclingStrategyType.BALANCED,
            verdicts = mapOf(gated.derivationIndex to CoinRecyclingState.TO_RECYCLE),
            expected = balanceOf(gainingPrivacy = 1.exponentToBalance(), canSpendWithConfirmation = true),
        )
    }

    private fun assertBalance(
        coins: List<Coin>,
        vouchers: List<RecyclerVoucher>,
        expected: CoinageBalance,
        coinStates: List<CoinageAssetState> = coins.map { CoinageAssetState.UNTRACKED },
        voucherStates: List<CoinageAssetState> = vouchers.map { CoinageAssetState.UNTRACKED },
        strategyType: RecyclingStrategyType = RecyclingStrategyType.MIN_PRIVACY,
        verdicts: RecyclingVerdicts = coins.associate { it.derivationIndex to CoinRecyclingState.ALLOW_USE },
    ) {
        assertEquals(expected, calculate(coins, vouchers, coinStates, voucherStates, strategyType, verdicts))
    }

    private fun calculate(
        coins: List<Coin>,
        vouchers: List<RecyclerVoucher>,
        coinStates: List<CoinageAssetState> = coins.map { CoinageAssetState.UNTRACKED },
        voucherStates: List<CoinageAssetState> = vouchers.map { CoinageAssetState.UNTRACKED },
        strategyType: RecyclingStrategyType = RecyclingStrategyType.MIN_PRIVACY,
        verdicts: RecyclingVerdicts = coins.associate { it.derivationIndex to CoinRecyclingState.ALLOW_USE },
    ): CoinageBalance = runBlocking {
        useCase.calculateCoinageBalance(
            coins = coins.zip(coinStates, ::TrackedCoin),
            vouchers = vouchers.zip(voucherStates, ::TrackedVoucher),
            strategyType = strategyType,
            verdicts = verdicts,
        ).getOrThrow()
    }

    private fun balanceOf(
        spendable: Balance = ZERO_BALANCE,
        gainingPrivacy: Balance = ZERO_BALANCE,
        pending: Balance = ZERO_BALANCE,
        canSpendWithConfirmation: Boolean = true,
    ) = CoinageBalance(
        availablePrivate = spendable,
        gainingPrivacy = CoinageBalance.GainingPrivacyBalance(gainingPrivacy, canSpendWithConfirmation),
        pending = pending,
    )

    private fun stateWithMinter(status: DurableTxStatus) =
        CoinageAssetState(handedOff = false, minterStatus = status, consumerStatus = null)

    private fun inRecycler(members: Int) = Location.InRecycler(RecyclerIndex(BigInteger.ONE), members, enteredAt = null)

    private fun coinOf(exponent: Int, age: Int?, onChain: Boolean = true, derivationIndex: Int = 0) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(exponent),
        age = age?.let(Coin.Age::Known) ?: Coin.Age.Unknown,
        isOnChain = onChain,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private fun voucherOf(exponent: Int, location: Location) = RecyclerVoucher(
        ringVrfKeyIndex = testKey(0),
        ringVrfPublicKey = mock(),
        recyclerValue = ValueExponent(exponent),
        location = location,
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private fun Int.exponentToBalance() = testConversionContext.formatExponentToBalance(ValueExponent(this))

    private fun List<Int>.exponentsToBalance() =
        map { testConversionContext.formatExponentToBalance(ValueExponent(it)) }
            .fold(ZERO_BALANCE) { acc, balance -> acc + balance }

    private companion object {
        val ZERO_BALANCE: Balance = BigInteger.ZERO.intoBalance()
    }
}
