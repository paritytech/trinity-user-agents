package io.paritytech.polkadotapp.feature_balances_api.domain.model

enum class BalancePreservation {
    /**
     * We do not want account's balance to become lower than ED
     */
    KEEP_ALIVE,

    /**
     * We do not care about account's balance becoming lower than ED
     */
    ALLOW_DEATH
}
