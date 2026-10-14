package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.totalBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts

/**
 * Rolls one classification of the user's holdings up into the three balances that partition their total.
 *
 * Public and free of any collaborator on purpose: the figures on screen and the sections of the bar beside
 * them have to come from the same arithmetic, and so does any synthetic holding a preview or a test-data
 * mode invents. Anything that can classify coins and vouchers can call this and get numbers that agree with
 * the real wallet's.
 *
 * A coin the evaluator has not judged yet counts as pending, not available. The correction that follows is
 * upward; the other way round would put spendable balance on screen and take it away a moment later.
 */
context(conversion: CoinageBalanceConversionContext)
fun coinageBalanceOf(
    coins: CoinBuckets,
    verdicts: RecyclingVerdicts,
    vouchers: VoucherBuckets,
    canSpendWithConfirmation: Boolean,
): CoinageBalance {
    val byVerdict = coins.minted.groupBy { verdicts[it.derivationIndex] }

    return CoinageBalance(
        availablePrivate = byVerdict.balanceOf(CoinRecyclingState.ALLOW_USE) + vouchers.usable.totalBalance(),
        gainingPrivacy = CoinageBalance.GainingPrivacyBalance(
            amount = byVerdict.balanceOf(CoinRecyclingState.TO_RECYCLE) + vouchers.gainingPrivacy.totalBalance(),
            canSpendWithConfirmation = canSpendWithConfirmation,
        ),
        pending = byVerdict.balanceOf(CoinRecyclingState.MUST_RECYCLE) +
            byVerdict.balanceWithoutVerdict() +
            coins.minting.totalBalance() +
            vouchers.minting.totalBalance(),
    )
}

/** A coin the evaluator has not judged yet keys to null, which is what puts it with the arriving money. */
context(conversion: CoinageBalanceConversionContext)
private fun Map<CoinRecyclingState?, List<Coin>>.balanceOf(state: CoinRecyclingState?) =
    this[state].orEmpty().totalBalance()

context(conversion: CoinageBalanceConversionContext)
private fun Map<CoinRecyclingState?, List<Coin>>.balanceWithoutVerdict() = balanceOf(state = null)
