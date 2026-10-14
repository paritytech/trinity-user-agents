package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingSnapshot
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.params
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import java.math.BigInteger

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

    private fun evaluate(
        type: RecyclingStrategyType,
        coins: List<Coin>,
        total: Balance,
        unavailable: Balance = Balance.ZERO,
    ): RecyclingVerdicts = runBlocking {
        val strategy = ParametricRecyclingStrategy(type.params, forcedAgeOf(FORCED_AGE))

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
}
