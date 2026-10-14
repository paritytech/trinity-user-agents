package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinRecyclingStrategy
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingSnapshot
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.params
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions

private const val FORCED_AGE = 14

/** The two limits that wrap every policy: what the chain will still accept, and what allowance is left. */
class RecyclingStrategyLimitsTest {
    private val quotaTracker: UnloadQuotaTracker = mock()

    private val forcedRecyclingAgeProvider = forcedAgeOf(FORCED_AGE)

    @Test
    fun `a coin at the forced age must recycle, whatever the policy said`() {
        val due = coinOf(age = FORCED_AGE)

        // Minimum privacy voluntarily gates nothing, so only the chain limit can act here.
        val verdicts = evaluate(chainLimited(RecyclingStrategyType.MIN_PRIVACY), listOf(due))

        assertEquals(CoinRecyclingState.MUST_RECYCLE, verdicts.getValue(due.derivationIndex))
    }

    @Test
    fun `a coin below the forced age keeps the policy verdict`() {
        val young = coinOf(age = FORCED_AGE - 1)

        val verdicts = evaluate(chainLimited(RecyclingStrategyType.MIN_PRIVACY), listOf(young))

        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(young.derivationIndex))
    }

    @Test
    fun `a coin of unknown age is never forced`() {
        val unknown = Coin(
            derivationIndex = testKey(0),
            valueExponent = ValueExponent(1),
            age = Coin.Age.Unknown,
            isOnChain = true,
            accountId = mock(),
            provenance = CoinProvenance.UNKNOWN,
        )

        val verdicts = evaluate(chainLimited(RecyclingStrategyType.MAX_PRIVACY), listOf(unknown))

        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(unknown.derivationIndex))
    }

    @Test
    fun `an exhausted allowance leaves only the coins the chain forces`() = runBlocking {
        whenever(quotaTracker.isQuotaRunningLow()).thenReturn(Result.success(true))

        val due = coinOf(age = FORCED_AGE, derivationIndex = 1)
        val discretionary = coinOf(age = 5, derivationIndex = 2)

        val verdicts = evaluate(fullChain(RecyclingStrategyType.MAX_PRIVACY), listOf(due, discretionary))

        assertEquals(CoinRecyclingState.MUST_RECYCLE, verdicts.getValue(due.derivationIndex))
        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(discretionary.derivationIndex))
    }

    /** A quota we could not read is not evidence of an exhausted one, so the user's choice stands. */
    @Test
    fun `an unreadable allowance does not stop recycling`() = runBlocking {
        whenever(quotaTracker.isQuotaRunningLow()).thenReturn(Result.failure(RuntimeException("chain down")))

        val discretionary = coinOf(age = 5)

        val verdicts = evaluate(fullChain(RecyclingStrategyType.MAX_PRIVACY), listOf(discretionary))

        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.getValue(discretionary.derivationIndex))
    }

    /**
     * The reason the mode exists: a lagging node must not hold the first balance back, so the pass that
     * produces it asks nothing of the chain — not even the allowance the quota tracker would have to read.
     */
    @Test
    fun `an immediate pass consults no chain limit`() {
        val discretionary = coinOf(age = 5)

        val verdicts = evaluate(
            strategy = fullChain(RecyclingStrategyType.MAX_PRIVACY),
            coins = listOf(discretionary),
            mode = BalanceEvaluationMode.IMMEDIATE,
        )

        // The policy would have gated this coin; it stays spendable until the complete pass says otherwise.
        assertEquals(CoinRecyclingState.ALLOW_USE, verdicts.getValue(discretionary.derivationIndex))
        verifyNoInteractions(quotaTracker)
    }

    @Test
    fun `an immediate pass still forces a coin the chain is about to refuse`() {
        val due = coinOf(age = FORCED_AGE)

        val verdicts = evaluate(
            strategy = fullChain(RecyclingStrategyType.MAX_PRIVACY),
            coins = listOf(due),
            mode = BalanceEvaluationMode.IMMEDIATE,
        )

        assertEquals(CoinRecyclingState.MUST_RECYCLE, verdicts.getValue(due.derivationIndex))
    }

    /**
     * The two recycling verdicts are not interchangeable: one is a privacy trade the user may take, the
     * other is a coin the chain will not accept. Only the first may ever be offered for spending.
     */
    @Test
    fun `the chain limit and the policy produce different verdicts`() {
        val forced = coinOf(age = FORCED_AGE, derivationIndex = 1)
        val chosen = coinOf(age = 5, derivationIndex = 2)

        val verdicts = evaluate(chainLimited(RecyclingStrategyType.MAX_PRIVACY), listOf(forced, chosen))

        assertEquals(CoinRecyclingState.MUST_RECYCLE, verdicts.getValue(forced.derivationIndex))
        assertEquals(CoinRecyclingState.TO_RECYCLE, verdicts.getValue(chosen.derivationIndex))
    }

    private fun chainLimited(type: RecyclingStrategyType) = EnsureChainLimitsStrategy(
        inner = ParametricRecyclingStrategy(type.params, forcedRecyclingAgeProvider),
        forcedRecyclingAgeProvider = forcedRecyclingAgeProvider,
    )

    private fun fullChain(type: RecyclingStrategyType) = EnsureChainLimitsStrategy(
        inner = EnsureQuotaLimitsStrategy(
            inner = ParametricRecyclingStrategy(type.params, forcedRecyclingAgeProvider),
            quotaTracker = quotaTracker,
        ),
        forcedRecyclingAgeProvider = forcedRecyclingAgeProvider,
    )

    private fun evaluate(
        strategy: CoinRecyclingStrategy,
        coins: List<Coin>,
        mode: BalanceEvaluationMode = BalanceEvaluationMode.COMPLETE,
    ): RecyclingVerdicts = runBlocking {
        val total = coins
            .map { testConversionContext.formatExponentToBalance(it.valueExponent) }
            .fold(Balance.ZERO) { acc, balance -> acc + balance }

        with(testConversionContext) {
            strategy.evaluate(coins, RecyclingSnapshot(total = total, unavailable = Balance.ZERO), mode)
        }
    }

    private fun coinOf(age: Int, derivationIndex: Int = 0) = Coin(
        derivationIndex = testKey(derivationIndex),
        valueExponent = ValueExponent(1),
        age = Coin.Age.Known(age),
        isOnChain = true,
        accountId = mock(),
        provenance = CoinProvenance.UNKNOWN,
    )
}
