package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId

sealed interface SendState {
    /** Submitted, but the coins are not on chain yet. */
    data object Detecting : SendState

    /** On chain and waiting for the recipient to take them. */
    data object Detected : SendState

    /** [unfinalizedCoins] are the coins whose claim is still only in a best block, or null when nothing is left to await. */
    data class Complete(val unfinalizedCoins: List<AccountId>?) : SendState

    data class Failed(val error: Throwable) : SendState
}
