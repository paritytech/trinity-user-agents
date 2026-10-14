package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ageOrNull
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinRecyclingStrategy
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingParams
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingSnapshot
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherUsabilityContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.resolveAgainst
import java.math.RoundingMode

/**
 * The only recycling policy there is. The named strategies differ by [params] alone, so an intermediate
 * position between them is a new set of numbers rather than a new class.
 */
class ParametricRecyclingStrategy(
    private val params: RecyclingParams,
    private val forcedRecyclingAgeProvider: ForcedRecyclingAgeProvider,
) : CoinRecyclingStrategy {
    context(conversion: CoinageBalanceConversionContext)
    override suspend fun evaluate(
        coins: List<Coin>,
        snapshot: RecyclingSnapshot,
        mode: BalanceEvaluationMode,
    ): RecyclingVerdicts {
        val budget = snapshot.total * params.maxUnavailableBalance.fraction
        val forcedRecyclingAge = forcedRecyclingAgeProvider.getForBalanceEvaluation(mode)
        val minRecyclingAge = params.minRecyclingAge.resolveAgainst(forcedRecyclingAge)
        var unavailable = snapshot.unavailable

        // Oldest first: a coin nearer the age the chain stops accepting has the most to lose by waiting,
        // so it gets first claim on the budget.
        return coins.sortedByDescending { it.ageOrNull() }.associate { coin ->
            val age = coin.ageOrNull()

            // Headroom, not fit. While any budget is left the next coin is admitted even if it overshoots,
            // so a coin worth more than the whole budget still recycles instead of sitting untouched until
            // the age limit forces it. The coin after an overshoot then finds no headroom and waits.
            val gated = age != null && age >= minRecyclingAge && unavailable < budget

            if (gated) unavailable += coin.balance()

            coin.derivationIndex to if (gated) CoinRecyclingState.TO_RECYCLE else CoinRecyclingState.ALLOW_USE
        }
    }

    override fun isVoucherUsable(voucher: RecyclerVoucher, context: VoucherUsabilityContext): Boolean {
        val location = voucher.location as? RecyclerVoucher.Location.InRecycler ?: return false

        val requiredMembers = context.capacityFor(voucher.recyclerValue)
            .toBigDecimal()
            .multiply(params.voucherReadiness.requiredRingFill.fraction)
            .setScale(0, RoundingMode.CEILING)
            .toInt()

        if (location.recyclerMembers >= requiredMembers) return true

        val requirements = params.voucherReadiness.memberAndAgeRequirements ?: return false
        val enteredAt = location.enteredAt ?: return false
        return location.recyclerMembers >= requirements.minimumMembers &&
            context.now - enteredAt >= requirements.delay
    }

    override fun allowsConfirmedSpend(): Boolean = params.allowsConfirmedSpend
}
