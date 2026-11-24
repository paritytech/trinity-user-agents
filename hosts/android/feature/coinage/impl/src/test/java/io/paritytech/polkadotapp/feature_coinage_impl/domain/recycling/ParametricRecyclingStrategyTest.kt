package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingSnapshot
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherUsabilityContext
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import java.math.BigInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.days
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

private const val FORCED_AGE = 14

class ParametricRecyclingStrategyTest {
    @Test
    fun `min privacy gates nothing, however old the coins`() {
        val coins = listOf(coinOf(age = 13, exponent = 1), coinOf(age = 10, exponent = 1, derivationIndex = 1))

        val verdicts = evaluate(RecyclingStrategyType.MIN_PRIVACY, coins, total = balanceOf(1, 1))

        assertEquals(coins.allowingAll(), verdicts)
    }

    @Test
    fun `max privacy gates every settled coin`() {
        val coins = listOf(coinOf(age = 1, exponent = 1), coinOf(age = 9, exponent = 1, derivationIndex = 1))

        val verdicts = evaluate(RecyclingStrategyType.MAX_PRIVACY, coins, total = balanceOf(1, 1))

        assertEquals(coins.gatingAll(), verdicts)
    }

    @Test
    fun `balanced leaves a coin below its age threshold alone`() {
        val young = coinOf(age = 3, exponent = 1)

        val verdicts = evaluate(RecyclingStrategyType.BALANCED, listOf(young), total = balanceOf(1))

        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(young.derivationIndex))
    }

    @Test
    fun `balanced gates a coin at its age threshold`() {
        val old = coinOf(age = 4, exponent = 1)

        val verdicts = evaluate(RecyclingStrategyType.BALANCED, listOf(old), total = balanceOf(1))

        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.getValue(old.derivationIndex))
    }

    /**
     * Headroom, not fit. A strict ceiling would leave a coin worth more than the whole budget untouched
     * until the chain's age limit forced it, which is the opposite of what the budget is for.
     */
    @Test
    fun `a coin larger than the whole budget is still admitted`() {
        val whale = coinOf(age = 5, exponent = 3)

        val verdicts = evaluate(RecyclingStrategyType.BALANCED, listOf(whale), total = balanceOf(3))

        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.getValue(whale.derivationIndex))
    }

    @Test
    fun `the coin after an overshoot finds no headroom`() {
        val first = coinOf(age = 9, exponent = 3, derivationIndex = 1)
        val second = coinOf(age = 8, exponent = 1, derivationIndex = 2)

        val verdicts = evaluate(RecyclingStrategyType.BALANCED, listOf(first, second), total = balanceOf(3, 1))

        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.getValue(first.derivationIndex))
        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(second.derivationIndex))
    }

    /** Oldest first, so the coin with the least time left gets the budget. */
    @Test
    fun `priority runs oldest first`() {
        val younger = coinOf(age = 5, exponent = 3, derivationIndex = 1)
        val older = coinOf(age = 12, exponent = 3, derivationIndex = 2)

        val verdicts = evaluate(RecyclingStrategyType.BALANCED, listOf(younger, older), total = balanceOf(3, 3))

        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.getValue(older.derivationIndex))
        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(younger.derivationIndex))
    }

    @Test
    fun `an empty wallet does not divide by zero`() {
        val verdicts = evaluate(RecyclingStrategyType.MAX_PRIVACY, emptyList(), total = Balance.ZERO)

        assertEquals(emptyMap<Int, CoinRecyclingState>(), verdicts)
    }

    /**
     * Required behaviour, not an implementation detail: a coin whose age nothing has read yet must never be
     * picked for recycling. Asserted for every preset because max privacy sits right on the boundary.
     */
    @Test
    fun `a coin of unknown age is never gated, under any preset`() {
        val unknown = Coin(
            derivationIndex = testKey(0),
            valueExponent = ValueExponent(1),
            age = Coin.Age.Unknown,
            isOnChain = true,
            accountId = mock(),
            provenance = CoinProvenance.UNKNOWN,
        )

        RecyclingStrategyType.entries.forEach { type ->
            val verdicts = evaluate(type, listOf(unknown), total = balanceOf(1))

            assertEquals(
                "unknown age was gated under $type",
                CoinRecyclingState.ALLOW_USE,
                verdicts.getValue(unknown.derivationIndex),
            )
        }
    }

    @Test
    fun `min privacy makes an in-recycler voucher usable at once whatever its members`() {
        listOf(0, 1, FULL_RING_CAPACITY).forEach { members ->
            val voucher = inRecycler(members = members, unloaded = members, enteredAt = null)

            assertTrue("$members members", RecyclingStrategyType.MIN_PRIVACY.isUsable(voucher, after = Duration.ZERO))
        }
    }

    @Test
    fun `a voucher is ready at the fill target`() {
        val cases = mapOf(
            RecyclingStrategyType.BALANCED to listOf(153 to false, 154 to true),
            RecyclingStrategyType.MAX_PRIVACY to listOf(690 to false, 691 to true),
        )

        cases.forEach { (type, boundaries) ->
            boundaries.forEach { (members, ready) ->
                val voucher = inRecycler(members = members, enteredAt = null)

                assertEquals("$type, $members members", ready, type.isUsable(voucher, after = Duration.ZERO))
            }
        }
    }

    @Test
    fun `the anonymity wait divides the budget by the members left`() {
        data class Case(val type: RecyclingStrategyType, val members: Int, val wait: Duration)
        val cases = listOf(
            Case(RecyclingStrategyType.BALANCED, members = 16, wait = 5.minutes),
            Case(RecyclingStrategyType.BALANCED, members = 8, wait = 10.minutes),
            Case(RecyclingStrategyType.BALANCED, members = 4, wait = 20.minutes),
            Case(RecyclingStrategyType.BALANCED, members = 1, wait = 80.minutes),
            Case(RecyclingStrategyType.BALANCED, members = 3, wait = 1_600_000.milliseconds),
            Case(RecyclingStrategyType.MAX_PRIVACY, members = 16, wait = 10.minutes),
        )

        cases.forEach { case ->
            val voucher = inRecycler(members = case.members)

            assertFalse("$case", case.type.isUsable(voucher, after = case.wait - 1.milliseconds))
            assertTrue("$case", case.type.isUsable(voucher, after = case.wait))
        }
    }

    @Test
    fun `a voucher is not ready before inclusion`() {
        listOf(RecyclingStrategyType.BALANCED, RecyclingStrategyType.MAX_PRIVACY).forEach { type ->
            listOf(Location.Onboarding, Location.Unknown).forEach { location ->
                assertFalse("$type, $location", type.isUsable(voucherOf(location), after = 1.days))
            }
        }

        val unconfirmed = inRecycler(members = 100, enteredAt = null)

        assertFalse(RecyclingStrategyType.BALANCED.isUsable(unconfirmed, after = 1.days))
    }

    @Test
    fun `more unloads than members never make a voucher ready by time`() {
        listOf(41, 40).forEach { unloaded ->
            val voucher = inRecycler(members = 40, unloaded = unloaded)

            assertFalse("$unloaded unloaded", RecyclingStrategyType.BALANCED.isUsable(voucher, after = 1.days))
        }
    }

    @Test
    fun `an unload in the ring delays readiness by time`() {
        assertTrue(RecyclingStrategyType.BALANCED.isUsable(inRecycler(members = 8, unloaded = 0), after = 10.minutes))
        assertFalse(RecyclingStrategyType.BALANCED.isUsable(inRecycler(members = 8, unloaded = 1), after = 10.minutes))
    }

    @Test
    fun `an unread ring capacity leaves the time route open`() {
        val voucher = inRecycler(members = 8)
        val unreadCapacity = { now: Instant -> FetchedVoucherUsabilityContext(ringCapacities = emptyMap(), now = now) }

        assertFalse(RecyclingStrategyType.BALANCED.isUsable(voucher, after = Duration.ZERO, context = unreadCapacity))
        assertTrue(RecyclingStrategyType.BALANCED.isUsable(voucher, after = 10.minutes, context = unreadCapacity))
    }

    @Test
    fun `the immediate context releases at the same fill target`() {
        val immediate = { now: Instant -> ImmediateVoucherUsabilityContext(now) }

        val ready = listOf(153, 154).map { members ->
            RecyclingStrategyType.BALANCED.isUsable(inRecycler(members, enteredAt = null), after = Duration.ZERO, context = immediate)
        }

        assertEquals(listOf(false, true), ready)
    }

    @Test
    fun `an unread unloaded count never makes a voucher ready by time`() {
        val voucher = inRecycler(members = 8, unloaded = null)

        listOf(RecyclingStrategyType.BALANCED, RecyclingStrategyType.MAX_PRIVACY).forEach { type ->
            assertFalse("$type", type.isUsable(voucher, after = 1.days))
        }
    }

    private fun RecyclingStrategyType.isUsable(
        voucher: RecyclerVoucher,
        after: Duration,
        context: (Instant) -> VoucherUsabilityContext = ::fullRingContext,
    ): Boolean = ParametricRecyclingStrategy(bundledParams, forcedAgeOf(FORCED_AGE))
        .isVoucherUsable(voucher, context(ENTERED_AT + after))

    private fun fullRingContext(now: Instant) =
        FetchedVoucherUsabilityContext(ringCapacities = mapOf(ValueExponent(1) to FULL_RING_CAPACITY), now = now)

    private fun inRecycler(members: Int, unloaded: Int? = 0, enteredAt: Instant? = ENTERED_AT) =
        voucherOf(Location.InRecycler(RecyclerIndex(BigInteger.ONE), members, enteredAt, unloaded))

    private fun evaluate(
        type: RecyclingStrategyType,
        coins: List<Coin>,
        total: Balance,
        unavailable: Balance = Balance.ZERO,
    ): RecyclingVerdicts = runBlocking {
        val strategy = ParametricRecyclingStrategy(type.bundledParams, forcedAgeOf(FORCED_AGE))

        with(testConversionContext) {
            strategy.evaluate(
                coins = coins,
                snapshot = RecyclingSnapshot(total = total, unavailable = unavailable),
                mode = BalanceEvaluationMode.COMPLETE,
            )
        }
    }

    private fun balanceOf(vararg exponents: Int): Balance = exponents
        .map { testConversionContext.formatExponentToBalance(ValueExponent(it)) }
        .fold(BigInteger.ZERO.intoBalance()) { acc, balance -> acc + balance }

    private fun List<Coin>.allowingAll() = associate { it.derivationIndex to CoinRecyclingState.ALLOW_USE }

    private fun List<Coin>.gatingAll() = associate { it.derivationIndex to CoinRecyclingState.TO_RECYCLE }

    private fun coinOf(age: Int, exponent: Int, derivationIndex: Int = 0) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(exponent),
        age = Coin.Age.Known(age),
        isOnChain = true,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )

    private companion object {
        val ENTERED_AT = Instant.fromEpochMilliseconds(0)
    }
}
