package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

/**
 * What the user holds, split by whether they can spend it right now.
 *
 * The middle bucket is the interesting one: it is money the strategy is deliberately holding back, which
 * some strategies will still let go of if the user says so. [pending] never will.
 */
data class CoinageBalance(
    val availablePrivate: Balance,
    val gainingPrivacy: GainingPrivacyBalance,
    /** On its way, or past the age the chain still accepts. Not spendable on any terms. */
    val pending: Balance,
) {
    companion object {
        val EMPTY = CoinageBalance(
            availablePrivate = Balance.ZERO,
            gainingPrivacy = GainingPrivacyBalance(
                amount = Balance.ZERO,
                canSpendWithConfirmation = true
            ),
            pending = Balance.ZERO
        )
    }

    data class GainingPrivacyBalance(
        val amount: Balance,
        /**
         * Whether the user may spend [amount] anyway once they have confirmed. The privacy it has earned so
         * far is lost if they do, which is why it takes a confirmation instead of being part of [availablePrivate].
         */
        val canSpendWithConfirmation: Boolean,
    )

    /**
     * Everything the chosen strategy will let the user part with, including what it is holding back but
     * would release on confirmation. [availablePrivate] alone is the subset that costs no privacy to spend.
     */
    val available: Balance = if (gainingPrivacy.canSpendWithConfirmation) {
        availablePrivate + gainingPrivacy.amount
    } else {
        availablePrivate
    }

    val total: Balance = availablePrivate + gainingPrivacy.amount + pending
}
